//! `studio.launcher`: Studio's project launcher
//! (`docs/design/project-launcher.md`).
//!
//! The vault's setlists in their sets, each song with what it is ready
//! with ([`model`]), the session found for it in the org's project root
//! ([`find`]), what the engine opened lately and the rigs on this machine;
//! a press opens a setlist or a song in Studio's engine (the `daw`
//! extension's `DAW_OPEN`). Everything it does is an action, so the
//! command palette, a key, the CLI and an agent open what the view opens.

pub mod find;
pub mod host;
pub mod model;
pub mod nav;
mod view;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use architect_extensions::linked::{Builder, Extension};
use serde_json::{Value, json};

use model::{Entry, Setlist};

pub const MANIFEST: &str = include_str!("../extension.toml");

/// The extension's id, and its view's.
pub const ID: &str = "studio.launcher";
pub const VIEW: &str = "launcher";

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
            .action("STUDIO_LAUNCHER_WINDOW", |_| async {
                host::run(
                    "WORKSPACE_OPEN",
                    json!({ "view": format!("{ID}/{VIEW}"), "title": "Launcher", "at": { "window": {} } }),
                )
                .await
            })
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
                open_setlist(found.named(&name)?).await
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

/// A setlist with the session found for each of its places, in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub setlist: Setlist,
    /// The setlist's own session file, when its note names one or its
    /// project root has one.
    pub file: Option<PathBuf>,
    pub sessions: Vec<Option<PathBuf>>,
    /// Whether each place has lighting cues in the project.
    pub lights: Vec<bool>,
}

impl Found {
    /// `setlist` over `vault_root` (none: nothing is found).
    #[must_use]
    pub fn of(setlist: Setlist, vault_root: Option<&Path>) -> Self {
        let root = vault_root.map(|v| (v, find::project_root(v, &setlist.project)));
        let session = |entry: &Entry| {
            let (vault, project) = root.as_ref()?;
            let song = entry.song.as_ref();
            song.and_then(|s| s.session.as_deref())
                .and_then(|written| find::resolve(written, vault, project))
                .or_else(|| {
                    let title = song.map_or(entry.label.as_str(), |s| s.title.as_str());
                    find::song_session(project, title)
                })
        };
        let sessions = setlist.entries().map(session).collect();
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
            lights,
        }
    }

    /// How many of its places are ready on every count: a chart, lyrics,
    /// a recording, tracks and lights.
    #[must_use]
    pub fn ready(&self) -> usize {
        self.setlist
            .entries()
            .zip(&self.sessions)
            .zip(&self.lights)
            .filter(|((entry, session), lights)| {
                session.is_some()
                    && **lights
                    && entry
                        .song
                        .as_ref()
                        .is_some_and(|s| s.chart && s.lyrics && (s.reference || s.recording))
            })
            .count()
    }

    /// How many of its places have a session.
    #[must_use]
    pub fn playable(&self) -> usize {
        self.sessions.iter().flatten().count()
    }

    fn json(&self) -> Value {
        let mut places = self.sessions.iter().zip(&self.lights);
        let sets: Vec<Value> = self
            .setlist
            .sets
            .iter()
            .map(|set| {
                let songs: Vec<Value> = set
                    .entries
                    .iter()
                    .map(|entry| {
                        let (session, lights) = places.next().unwrap_or((&None, &false));
                        model::entry_json(entry, session.as_deref().and_then(Path::to_str), *lights)
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
            "file": self.file,
            "length": length,
            "length_unknown": unknown,
            "playable": self.playable(),
            "ready": self.ready(),
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

/// Opens `found` in the engine: its own file, else a setlist file written
/// from the songs that have a session.
///
/// # Errors
///
/// None of its songs has a session, or the engine refused.
pub async fn open_setlist(found: &Found) -> Result<Value, String> {
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
    let said = host::run("DAW_OPEN", json!({ "path": path })).await?;
    Ok(json!({ "opened": path, "setlist": title, "left_out": left_out, "engine": said }))
}

/// Opens place `at` of `found` in the engine: gone to when the engine's
/// open set has that song, else its session opened on its own.
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
    // The engine's set already has it: go there, the set stays open.
    if let Some(number) = in_open_set(path).await {
        let said = host::engine("SESSION_SONG", json!({ "to": number })).await?;
        return Ok(json!({ "went_to": number, "song": entry.label, "engine": said }));
    }
    let said = host::run("DAW_OPEN", json!({ "path": path })).await?;
    Ok(json!({ "opened": path, "song": entry.label, "engine": said }))
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
