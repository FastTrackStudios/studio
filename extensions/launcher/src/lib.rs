//! `studio.launcher`: Studio's project launcher
//! (`docs/design/project-launcher.md`).
//!
//! The vault's setlists in their sets ([`model`]), the session found for
//! each song in the org's project root ([`find`]), what the engine opened
//! lately and the rigs on this machine ([`nav`]: its section of
//! Architect's navigator). A press opens a setlist or a song in Studio's
//! engine (the `daw` extension's `DAW_OPEN`), lays the panes out for it
//! ([`arrange`], [`layout`]) and points the chart and the lighting at the
//! song ([`follow`]). What a setlist keeps about itself (a note per song,
//! its rigs, its layout) is written to its note ([`plan`]). Everything it
//! does is an action, so the command palette, a key, the CLI and an agent
//! open what the view opens.

// In a browser the host's futures are not `Send` (one thread: the
// host's `MaybeSend`), so neither are the launcher's that await them.
#![cfg_attr(target_arch = "wasm32", allow(clippy::future_not_send))]

pub mod arrange;
pub mod find;
pub mod follow;
pub mod host;
pub mod layout;
pub mod model;
pub mod nav;
pub mod plan;
mod view;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use architect_extensions::linked::{Builder, Extension};
use serde_json::{Value, json};

use model::{Entry, Setlist};

pub const MANIFEST: &str = include_str!("../extension.toml");

/// The extension's id, and its view's.
pub const ID: &str = "studio.launcher";
pub const VIEW: &str = "launcher";

/// The launcher's pane: a single view, so its pane's id is its view's.
pub const PANE: &str = "studio.launcher/launcher";

/// The setlist the view shows (its note's path): chosen in the navigator,
/// in the view, or by `STUDIO_LAUNCHER_CHOOSE`.
static CHOSEN: Mutex<Option<String>> = Mutex::new(None);

/// The setlist the view shows, when one was chosen.
#[must_use]
pub fn chosen() -> Option<String> {
    CHOSEN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Shows setlist `path` in the view.
pub fn choose(path: String) {
    *CHOSEN.lock().unwrap_or_else(PoisonError::into_inner) = Some(path);
}

pub struct Launcher;

impl Extension for Launcher {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    #[allow(clippy::too_many_lines)] // One line an action: the list is the extension.
    fn build(&self, ext: &mut Builder) {
        host::connect(ext.handle());
        ext.navigator(nav::PREFIX, nav::LauncherNav);
        ext.view(VIEW, view::LauncherView)
            .action("STUDIO_LAUNCHER_CHOOSE", |args| async move {
                let name = text(&args, "setlist")?;
                let found = load().await?;
                let list = &found.named(&name)?.setlist;
                choose(list.path.clone());
                host::run("EXT_VIEW_OPEN", json!({ "extension": ID, "view": VIEW })).await?;
                Ok(json!({ "showing": list.title }))
            })
            .action("STUDIO_LAUNCHER_SHOW", |_| async {
                host::run("EXT_VIEW_OPEN", json!({ "extension": ID, "view": VIEW })).await
            })
            .action("STUDIO_LAUNCHER_WINDOW", |_| window())
            .action("STUDIO_LAUNCHER_STARTUP", |_| startup())
            .action("STUDIO_LAUNCHER_LIST", |_| async {
                let found = load().await?;
                Ok(json!({
                    "vault": found.vault_root,
                    "setlists": found.setlists.iter().map(Found::json).collect::<Vec<_>>(),
                }))
            })
            .action("STUDIO_LAUNCHER_OPEN_SETLIST", |args| async move {
                let name = text(&args, "setlist")?;
                let found = load().await?;
                open_setlist(found.named(&name)?, opt(&args, "layout").as_deref()).await
            })
            .action("STUDIO_LAUNCHER_OPEN_SONG", |args| async move {
                let name = text(&args, "song")?;
                let found = load().await?;
                let (list, at) = found.song(&name, args.get("setlist").and_then(Value::as_str))?;
                open_song(list, at).await
            })
            .action("STUDIO_LAUNCHER_RECENT", |_| async {
                let recent = recent().await?;
                Ok(json!({
                    "recent": recent.iter().map(|r| json!({
                        "name": r.name, "when": r.when, "path": r.path,
                    })).collect::<Vec<_>>(),
                }))
            })
            .action("STUDIO_LAUNCHER_ARRANGE", |args| async move {
                let mut said = arrange::arrange(&text(&args, "layout")?).await?;
                // The chart and the lighting of the panes just laid out.
                follow::forget();
                let pointed = follow::follow(true)
                    .await
                    .unwrap_or_else(|why| json!({ "refused": why }));
                if let Some(fields) = said.as_object_mut() {
                    fields.insert("pointed".to_owned(), pointed);
                }
                Ok(said)
            })
            .action("STUDIO_LAUNCHER_LAYOUT_SAVE", |args| async move {
                arrange::save(&text(&args, "name")?).await
            })
            .action("STUDIO_LAUNCHER_LAYOUTS", |_| async {
                Ok(json!({ "made": layout::BUILT_IN, "saved": arrange::saved_names().await }))
            })
            .action("STUDIO_LAUNCHER_FOLLOW", |args| async move {
                // Its hook hands it the deck's event; a person or a script
                // asks with `force`.
                follow::follow(args.get("force").and_then(Value::as_bool).unwrap_or(false)).await
            })
            .action("STUDIO_LAUNCHER_NOTE", |args| async move {
                let found = load().await?;
                let list = found.named(&text(&args, "setlist")?)?;
                plan::note(
                    list,
                    &text(&args, "song")?,
                    &opt(&args, "note").unwrap_or_default(),
                )
                .await
            })
            .action("STUDIO_LAUNCHER_PART", |args| async move {
                let found = load().await?;
                let list = found.named(&text(&args, "setlist")?)?;
                let (player, rig) = (
                    opt(&args, "player").unwrap_or_default(),
                    text(&args, "rig")?,
                );
                let (song, patch) = (opt(&args, "song"), opt(&args, "patch").unwrap_or_default());
                let edit = plan::PartEdit {
                    player: &player,
                    rig: &rig,
                    profile: args.get("profile").and_then(Value::as_str),
                    patch: song.as_deref().map(|s| (s, patch.as_str())),
                    remove: args.get("remove").and_then(Value::as_bool).unwrap_or(false),
                };
                plan::part(list, &edit).await
            })
            .action("STUDIO_LAUNCHER_NAME_SESSIONS", |args| async move {
                let found = load().await?;
                let root = found
                    .vault_root
                    .clone()
                    .ok_or("the vault has no folder on disk")?;
                plan::name_sessions(found.named(&text(&args, "setlist")?)?, Path::new(&root)).await
            })
            .action("STUDIO_LAUNCHER_RIGS", |_| async {
                Ok(json!({
                    "rigs": find::rigs().iter().map(|r| json!({
                        "instrument": r.instrument, "profiles": r.profiles,
                    })).collect::<Vec<_>>(),
                    "loads": false,
                    "why": "loading a rig into Studio's engine is not built yet: \
                            the engine's RIG_LOAD is compiled out of the daw extension \
                            (docs/design/project-launcher.md, Rigs)",
                }))
            });
    }
}

fn text(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("give `{key}`"))
}

fn opt(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
}

/// Opens the launcher in a window of its own: opened there, or moved
/// there when it is already a pane.
async fn window() -> Result<Value, String> {
    let to = json!({ "window": {} });
    match host::run(
        "WORKSPACE_OPEN",
        json!({ "view": PANE, "title": "Launcher", "at": to }),
    )
    .await
    {
        Ok(out) => Ok(out),
        Err(_) => host::run("WORKSPACE_MOVE", json!({ "pane": PANE, "to": to })).await,
    }
}

/// Whether the launcher has a window to itself in `layout` (a
/// `WORKSPACE_LAYOUT` answer): a window other than the main one whose
/// only pane it is.
#[must_use]
pub fn has_own_window(layout: &Value) -> bool {
    layout
        .get("windows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|w| w.get("id").and_then(Value::as_str) != Some("main"))
        .any(|w| {
            let panes: Vec<&str> = w
                .get("groups")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .flat_map(|g| {
                    g.get("tabs")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                })
                .filter_map(|t| t.as_str().or_else(|| t.get("pane").and_then(Value::as_str)))
                .collect();
            panes == [PANE]
        })
}

/// Closes the launcher when it has a window to itself: something was
/// opened from it. The launcher as a pane among others stays.
async fn close_window() {
    let own = host::run("WORKSPACE_LAYOUT", json!({}))
        .await
        .is_ok_and(|layout| has_own_window(&layout));
    if own {
        let _ = host::run("WORKSPACE_CLOSE", json!({ "panes": [PANE] })).await;
    }
}

/// How long launch waits for the engine to say what it has.
const ENGINE_WAIT: Duration = Duration::from_secs(8);
/// How long a set being opened at launch is waited for.
const OPENING_WAIT: Duration = Duration::from_secs(90);
const TICK: Duration = Duration::from_millis(500);

/// What the engine has at a moment of launch.
enum AtLaunch {
    /// It has a song: the app comes back as it was.
    Song,
    /// It is opening one (`FTS_DAW_OPEN`, the last set).
    Opening,
    /// It is up and has nothing.
    Nothing,
    /// It does not answer (yet).
    Silent,
}

async fn at_launch() -> AtLaunch {
    let Ok(state) = host::session("state", json!({})).await else {
        return AtLaunch::Silent;
    };
    if follow::Session::of(&state).is_some() {
        return AtLaunch::Song;
    }
    if state.get("up").and_then(Value::as_bool) != Some(true) {
        return AtLaunch::Silent;
    }
    let opening = host::engine("SESSION_OPENING", json!({})).await.ok();
    let has_set = host::engine("SESSION_SONGS", json!({}))
        .await
        .ok()
        .and_then(|s| {
            s.get("songs")
                .unwrap_or(&s)
                .as_array()
                .map(|a| !a.is_empty())
        })
        .unwrap_or(false);
    match opening
        .as_ref()
        .and_then(|o| o.get("state"))
        .and_then(Value::as_str)
    {
        Some("opening") => AtLaunch::Opening,
        _ if has_set => AtLaunch::Song,
        _ => AtLaunch::Nothing,
    }
}

/// At launch: the launcher's window when the engine has nothing open, and
/// nothing when it has (the app comes back as it was).
///
/// The engine is a process of its own and comes up after the window, so
/// it is asked for a while: a song there ends the wait at once; an engine
/// that is up with nothing for three asks, or that never answers, shows
/// the launcher.
async fn startup() -> Result<Value, String> {
    let mut waited = Duration::ZERO;
    let mut idle = 0u8;
    loop {
        let limit = match at_launch().await {
            AtLaunch::Song => {
                // What the engine opened at launch gets its chart and lighting too.
                let _ = follow::follow(true).await;
                return Ok(json!({ "shown": false, "why": "the engine has a song" }));
            }
            AtLaunch::Opening => {
                idle = 0;
                OPENING_WAIT
            }
            AtLaunch::Nothing => {
                idle = idle.saturating_add(1);
                if idle >= 3 {
                    break;
                }
                ENGINE_WAIT
            }
            AtLaunch::Silent => ENGINE_WAIT,
        };
        if waited >= limit {
            break;
        }
        futures_timer::Delay::new(TICK).await;
        waited = waited.saturating_add(TICK);
    }
    window().await?;
    Ok(json!({ "shown": true, "why": "the engine has nothing open" }))
}

/// A setlist with the session found for each of its places, in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub setlist: Setlist,
    /// The setlist's own session file, when its note names one or its
    /// project root has one.
    pub file: Option<PathBuf>,
    pub sessions: Vec<Option<PathBuf>>,
    /// Whether each place's session is the one its song's note names
    /// (`session`); otherwise it was found by the song's name.
    pub named: Vec<bool>,
    /// Whether each place has lighting cues in the project.
    pub lights: Vec<bool>,
}

impl Found {
    /// `setlist` over `vault_root` (none: nothing is found on disk; in a
    /// browser, where a vault has no folder, the engine in the page opens
    /// its notes: [`Found::by_notes`]).
    #[must_use]
    pub fn of(setlist: Setlist, vault_root: Option<&Path>) -> Self {
        if vault_root.is_none() && cfg!(target_arch = "wasm32") {
            return Self::by_notes(setlist);
        }
        let root = vault_root.map(|v| (v, find::project_root(v, &setlist.project)));
        // The session its note names, else the one found by its name.
        let session = |entry: &Entry| -> (Option<PathBuf>, bool) {
            let Some((vault, project)) = root.as_ref() else {
                return (None, false);
            };
            let song = entry.song.as_ref();
            let written = song
                .and_then(|s| s.session.as_deref())
                .and_then(|written| find::resolve(written, vault, project));
            if written.is_some() {
                return (written, true);
            }
            let title = song.map_or(entry.label.as_str(), |s| s.title.as_str());
            (find::song_session(project, title), false)
        };
        let (sessions, named) = setlist.entries().map(session).unzip();
        let lights = setlist
            .entries()
            .map(|e| {
                root.as_ref().is_some_and(|(vault, _)| {
                    find::cues(vault, &setlist.project, &e.label).is_some()
                })
            })
            .collect();
        let file = root.as_ref().and_then(|(vault, project)| {
            setlist
                .session
                .as_deref()
                .and_then(|written| find::resolve(written, vault, project))
                .or_else(|| find::setlist_file(project))
        });
        Self {
            setlist,
            file,
            sessions,
            named,
            lights,
        }
    }

    /// `setlist` opened by its notes: what a browser's engine opens, which
    /// reads a song from its note (its recording, its stems, its chart)
    /// and a set from the setlist's. Each place whose song note is in the
    /// vault is its note's path; the setlist's own "file" is its note.
    /// Nothing has lighting cues (they are files of the project, on disk).
    #[must_use]
    pub fn by_notes(setlist: Setlist) -> Self {
        let sessions: Vec<Option<PathBuf>> = setlist
            .entries()
            .map(|e| e.song.as_ref().map(|s| PathBuf::from(&s.path)))
            .collect();
        let named = sessions.iter().map(Option::is_some).collect();
        let lights = sessions.iter().map(|_| false).collect();
        Self {
            file: Some(PathBuf::from(&setlist.path)),
            setlist,
            sessions,
            named,
            lights,
        }
    }

    /// How many of its places have a session.
    #[must_use]
    pub fn playable(&self) -> usize {
        self.sessions.iter().flatten().count()
    }

    fn json(&self) -> Value {
        let mut places = self.sessions.iter().zip(&self.named).zip(&self.lights);
        let sets: Vec<Value> = self
            .setlist
            .sets
            .iter()
            .map(|set| {
                let songs: Vec<Value> = set
                    .entries
                    .iter()
                    .map(|entry| {
                        let ((session, named), lights) =
                            places.next().unwrap_or(((&None, &false), &false));
                        let mut song = model::entry_json(
                            entry,
                            session.as_deref().and_then(Path::to_str),
                            *lights,
                        );
                        if let (Some(fields), true) = (song.as_object_mut(), session.is_some()) {
                            let from = if *named { "note" } else { "name" };
                            fields.insert("session_from".to_owned(), json!(from));
                        }
                        song
                    })
                    .collect();
                json!({ "set": set.name, "songs": songs })
            })
            .collect();
        let (length, unknown) = self.setlist.length();
        json!({
            "title": self.setlist.title,
            "path": self.setlist.path,
            "date": self.setlist.date,
            "project": self.setlist.project,
            "layout": self.setlist.layout,
            "parts": self.setlist.parts.iter().map(model::Part::json).collect::<Vec<_>>(),
            "file": self.file,
            "length": length,
            "length_unknown": unknown,
            "playable": self.playable(),
            "sets": sets,
        })
    }
}

/// What the vault has, as the launcher shows it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Loaded {
    pub vault_root: Option<String>,
    pub setlists: Vec<Found>,
}

impl Loaded {
    fn named(&self, name: &str) -> Result<&Found, String> {
        self.setlists
            .iter()
            .find(|f| f.setlist.path == name || f.setlist.title.eq_ignore_ascii_case(name.trim()))
            .ok_or_else(|| {
                format!("no setlist `{name}` in the vault (STUDIO_LAUNCHER_LIST lists them)")
            })
    }

    /// Song `name` (its title, its place's label or its note) and where it
    /// is in a setlist: `setlist`'s when given, else the first that has it.
    fn song(&self, name: &str, setlist: Option<&str>) -> Result<(&Found, usize), String> {
        let lists: Vec<&Found> = match setlist {
            Some(s) => vec![self.named(s)?],
            None => self.setlists.iter().collect(),
        };
        let is = |e: &Entry| {
            e.note == name
                || e.label.eq_ignore_ascii_case(name.trim())
                || e.song
                    .as_ref()
                    .is_some_and(|s| s.title.eq_ignore_ascii_case(name.trim()))
        };
        lists
            .into_iter()
            .find_map(|f| f.setlist.entries().position(is).map(|at| (f, at)))
            .ok_or_else(|| format!("no song `{name}` in a setlist of the vault"))
    }
}

/// Reads the vault's setlists and songs and finds their sessions.
///
/// # Errors
///
/// The vault's index refused the query.
pub async fn load() -> Result<Loaded, String> {
    let (setlists, songs) = host::notes().await?;
    let vault_root = host::vault_root().await;
    let root = vault_root.as_deref().map(Path::new);
    Ok(Loaded {
        setlists: model::setlists(&setlists, &songs)
            .into_iter()
            .map(|s| Found::of(s, root))
            .collect(),
        vault_root,
    })
}

/// How long an open that changes nothing is waited on: the song asked
/// for is the one the engine has.
const SAME_WAIT: Duration = Duration::from_secs(6);

/// Waits for the engine to have a song other than `before` (an open is
/// answered at once and carried out after): the session it has then.
/// When it stays on `before` and is not opening anything, that one: the
/// song asked for was the one it had.
async fn opened(before: Option<&str>) -> Option<follow::Session> {
    let mut waited = Duration::ZERO;
    while waited < OPENING_WAIT {
        let now = follow::session().await;
        if let Some(now) = now.clone().filter(|s| Some(s.id.as_str()) != before) {
            return Some(now);
        }
        if waited >= SAME_WAIT {
            let opening = host::engine("SESSION_OPENING", json!({})).await.ok();
            let busy = opening
                .as_ref()
                .and_then(|o| o.get("state"))
                .and_then(Value::as_str)
                == Some("opening");
            if !busy {
                return now;
            }
        }
        futures_timer::Delay::new(TICK).await;
        waited = waited.saturating_add(TICK);
    }
    None
}

/// Opens `found` in the engine, lays the panes out and points the chart
/// and the lighting at its song.
///
/// Its own file is opened, else a setlist file written from the songs
/// that have a session. `layout` is applied when given, else the one the
/// setlist was opened with last (its note's `layout`), else none; a
/// layout given is remembered in the note.
///
/// # Errors
///
/// None of its songs has a session, or the engine refused.
pub async fn open_setlist(found: &Found, layout: Option<&str>) -> Result<Value, String> {
    let title = &found.setlist.title;
    let left_out: Vec<&str> = found
        .setlist
        .entries()
        .zip(&found.sessions)
        .filter(|(_, s)| s.is_none())
        .map(|(e, _)| e.label.as_str())
        .collect();
    let (path, left_out) = if let Some(file) = &found.file {
        (file.clone(), Vec::new())
    } else {
        let sessions: Vec<PathBuf> = found.sessions.iter().flatten().cloned().collect();
        if sessions.is_empty() {
            return Err(format!(
                "{title}: none of its songs has a session in {} yet",
                found.setlist.project
            ));
        }
        let file = find::write_setlist(title, &sessions).map_err(|e| e.to_string())?;
        (file, left_out)
    };
    let before = follow::session().await.map(|s| s.id);
    let said = host::session("open", json!({ "path": path })).await?;
    choose(found.setlist.path.clone());
    let layout = layout.or(found.setlist.layout.as_deref());
    let (arranged, pointed) = if opened(before.as_deref()).await.is_some() {
        let arranged = match layout {
            Some(name) => match arrange::arrange(name).await {
                Ok(out) => {
                    // The layout it was opened in is the one it opens in next.
                    let _ = plan::layout(found, name).await;
                    out
                }
                Err(why) => json!({ "layout": name, "refused": why }),
            },
            None => Value::Null,
        };
        follow::forget();
        let pointed = follow::follow(true)
            .await
            .unwrap_or_else(|why| json!({ "refused": why }));
        (arranged, pointed)
    } else {
        (Value::Null, Value::Null)
    };
    close_window().await;
    Ok(json!({
        "opened": path, "setlist": title, "left_out": left_out, "engine": said,
        "arranged": arranged, "pointed": pointed,
    }))
}

/// Opens place `at` of `found` in the engine: gone to when the engine's
/// open set has that song, else its session opened on its own. Its chart
/// and lighting follow.
///
/// # Errors
///
/// The song has no session, or the engine refused.
pub async fn open_song(found: &Found, at: usize) -> Result<Value, String> {
    let entry = found
        .setlist
        .entries()
        .nth(at)
        .ok_or("no such place in the setlist")?;
    let path = found
        .sessions
        .get(at)
        .and_then(Option::as_ref)
        .ok_or_else(|| {
            format!(
                "{}: no session for it in {} yet",
                entry.label, found.setlist.project
            )
        })?;
    let before = follow::session().await.map(|s| s.id);
    // The engine's set already has it: go there, the set stays open.
    let engine = if let Some(number) = in_open_set(path).await {
        host::engine("SESSION_SONG", json!({ "to": number }))
            .await
            .map(|said| json!({ "went_to": number, "song": entry.label, "engine": said }))
    } else {
        host::session("open", json!({ "path": path }))
            .await
            .map(|said| json!({ "opened": path, "song": entry.label, "engine": said }))
    };
    // In a browser the song's chart shows, whatever the engine said: its
    // chart follows the app's playhead there, not a session.
    let charted = if cfg!(target_arch = "wasm32") {
        show_chart(entry).await
    } else {
        None
    };
    // In a browser a song opened is its note and its chart: an engine
    // that refuses (none in the page yet) is said, not a failure.
    let mut said = match (engine, &charted) {
        (Ok(said), _) => said,
        (Err(why), Some(chart)) if cfg!(target_arch = "wasm32") => {
            return Ok(json!({ "song": entry.label, "chart": chart, "engine_refused": why }));
        }
        (Err(why), _) => return Err(why),
    };
    choose(found.setlist.path.clone());
    if let (Some(chart), Some(fields)) = (charted, said.as_object_mut()) {
        fields.insert("chart".to_owned(), chart);
    }
    if opened(before.as_deref()).await.is_some() {
        follow::forget();
        let pointed = follow::follow(true)
            .await
            .unwrap_or_else(|why| json!({ "refused": why }));
        if let Some(fields) = said.as_object_mut() {
            fields.insert("pointed".to_owned(), pointed);
        }
    }
    close_window().await;
    Ok(said)
}

/// Opens Keyflow's chart of `entry`'s song in a tab (its note's
/// `chart`, a vault path): what a browser shows of a song opened, where
/// the chart follows the app's playhead rather than a session's
/// (`follow` points a desk's chart at the engine's session instead).
async fn show_chart(entry: &Entry) -> Option<Value> {
    let song = entry.song.as_ref()?;
    let chart = song.chart_path.clone()?;
    // The song's note first, as a press in the navigator opens it (the
    // web app's home steps aside for it), then its chart in a tab of its
    // own, by the app's opener for a `.kf` (Keyflow's chart view,
    // `[[openers]]`).
    let note = host::open(&song.path).await;
    // The page draws the note (and the home goes) before the chart is
    // opened beside it: asked at once, the chart's tab is lost under it.
    futures_timer::Delay::new(Duration::from_millis(400)).await;
    Some(match host::open(&chart).await {
        Ok(_) => json!({ "chart": chart, "note": note.is_ok() }),
        Err(why) => json!({ "chart": chart, "refused": why }),
    })
}

/// The number (from 1) of the song whose session is `path` in the set the
/// engine has open.
async fn in_open_set(path: &Path) -> Option<u64> {
    let songs = host::engine("SESSION_SONGS", json!({})).await.ok()?;
    let name = path.file_stem()?.to_str()?;
    let list = songs.get("songs").unwrap_or(&songs).as_array()?;
    (list.len() > 1)
        .then(|| {
            list.iter().find(|s| {
                s.get("title")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t.eq_ignore_ascii_case(name))
            })
        })
        .flatten()?
        .get("number")
        .and_then(Value::as_u64)
}

/// What the engine opened lately.
///
/// # Errors
///
/// The engine is not running.
pub async fn recent() -> Result<Vec<model::Recent>, String> {
    Ok(model::recents(
        &host::engine("SESSION_RECENT", json!({})).await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setlist_opened_by_its_notes_opens_each_song_note_it_has() {
        let row = |path: &str, values: Value| model::Row {
            path: path.to_owned(),
            values: values.as_object().cloned().unwrap_or_default(),
        };
        let setlists = [row(
            "Setlists/Show.md",
            json!({ "title": "Show", "songs": ["[[Libraries/L/A/A|A]]", "[[Libraries/L/Gone/Gone|Gone]]"] }),
        )];
        let songs = [row(
            "Libraries/L/A/A.md",
            json!({ "title": "A", "type": "song" }),
        )];
        let list = model::setlists(&setlists, &songs).remove(0);
        let found = Found::by_notes(list);
        assert_eq!(found.file.as_deref(), Some(Path::new("Setlists/Show.md")));
        assert_eq!(
            found.sessions,
            [Some(PathBuf::from("Libraries/L/A/A.md")), None]
        );
        assert_eq!(found.playable(), 1);
        assert!(found.lights.iter().all(|l| !l));
    }

    #[test]
    fn the_launcher_has_a_window_to_itself_only_alone_and_off_the_main_one() {
        let layout = |main: Value, other: Value| {
            json!({ "windows": [
                { "id": "main", "groups": [ { "tabs": main } ] },
                { "id": "w2", "groups": [ { "tabs": other } ] },
            ] })
        };
        assert!(has_own_window(&layout(json!(["files"]), json!([PANE]))));
        assert!(has_own_window(&layout(
            json!([]),
            json!([{ "pane": PANE }])
        )));
        // A pane among others, or the main window's only pane, stays.
        assert!(!has_own_window(&layout(
            json!(["files"]),
            json!([PANE, "studio.mixer/mixer"])
        )));
        assert!(!has_own_window(&layout(json!([PANE]), json!([]))));
    }
}
