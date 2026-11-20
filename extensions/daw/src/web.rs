//! The `daw` extension in a browser: Studio's engine in the page.
//!
//! The web app links this build (studio-web, `studio_extensions::stage`).
//! There is no process: the engine is session-engine's web engine
//! (`session_engine::web_engine`, one `Standalone` in the page, its audio
//! out through a worklet), opened once for the page and hosted by
//! `session_ui::web_host`, whose state every Studio view reads
//! (`studio_view_link`). What it opens:
//!
//! - **A share link's set**, when the page is one (`/s/<code>/…`): opened
//!   behind the page as it loads, each song's renditions streamed by range
//!   from the link's own file URLs (`WebSource::Link`).
//! - What `DAW_OPEN` names: a share link (`url`).
//!
//! **One clock.** The engine is given to the app as the playhead's clock
//! (`architect_playhead::engine`, [`Engine`]): for an item whose media is
//! in a song the engine has open, the app's player loads, plays, seeks and
//! reads the engine, and holds nothing itself. A press in the engine's own
//! panels (the arrangement's Space) moves the engine first; [`watch`] hands
//! it to the deck as the app's own press, so the two never disagree.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use architect_extensions::linked::{Builder, Extension};
use architect_playhead::{Clock, Reading};
use serde_json::{Value as Json, json};
use session_engine::engine::{Move, Transport};
use session_engine::web_engine::{self, WebSource};
use session_ui::web_host::{self, HostedState};

pub struct Daw;

impl Extension for Daw {
    fn manifest(&self) -> &'static str {
        crate::MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        install();
        ext.action_in("DAW_READ", |_, _| async { Err(desktop_only("fts.session")) })
            .action_in("DAW_WRITE", |_, _| async { Err(desktop_only("fts.session")) })
            .action_in("DAW_STRUCTURE", |_, _| async {
                Err(desktop_only("fts.session"))
            })
            .action("DAW_STATE", |_| async { Ok(state()) })
            .action("DAW_OPEN", |args| async move { open(&args) })
            .action("DAW_AUDIO", |_| async {
                Ok(json!({
                    "device": { "output_device": "the browser's output", "sample_rate": 48_000 },
                    "devices": [],
                }))
            })
            .action("DAW_DECK", |_| async {
                // In a page the engine is the playhead's clock: it is told
                // by the app's player itself, not by this hook.
                Ok(json!({ "ok": true, "clock": "engine" }))
            })
            .action("DAW_RUN", |args| async move {
                let id = args.get("id").and_then(Json::as_str).ok_or("no id")?;
                if args.get("song").is_some_and(|s| !s.is_null()) {
                    return Err("DAW_RUN's `song` is the desktop engine's: in a browser an action runs on the song on screen".to_owned());
                }
                let none = Json::Null;
                studio_view_link::call(id, args.get("args").unwrap_or(&none)).await
            })
            .action("DAW_SONGS", |_| async {
                let open = web_host::hosted_is_open();
                let set = web_host::hosted_songs();
                Ok(json!({
                    "at": open.then_some(set.at.saturating_add(1)),
                    "songs": open.then(|| set.songs.iter().enumerate().map(|(i, song)| {
                        json!({ "number": i.saturating_add(1), "title": song.title(), "project": song.project })
                    }).collect::<Vec<_>>()),
                }))
            });
    }
}

fn desktop_only(what: &str) -> String {
    format!(
        "{what} is the desktop engine's for now: the page's engine plays and draws the set, it keeps no session document"
    )
}

/// What the page's engine is doing.
fn state() -> Json {
    let (state, failed) = match web_host::hosted() {
        HostedState::Idle => ("idle", None),
        HostedState::Opening(_) => ("opening", None),
        HostedState::Open(_) => ("open", None),
        HostedState::Failed(why) => ("failed", Some(why)),
    };
    let up = state == "open";
    let (time, moving) = reading();
    json!({
        "up": up,
        "where": "page",
        "state": state,
        "failed": failed,
        "songs": web_host::hosted_songs().songs.len(),
        "listening": up.then(|| format!("{:?}", web_engine::listening())),
        "clock": architect_playhead::engine::is_clock(),
        "time": time,
        "playing": moving,
    })
}

/// `DAW_OPEN`: a share link's set (`url`, or a `path` that is one).
fn open(args: &Json) -> Result<Json, String> {
    let named = args
        .get("url")
        .or_else(|| args.get("path"))
        .and_then(Json::as_str)
        .ok_or("`url`: a share link to open")?;
    session_runtime::link::listing_url(named)
        .map_err(|e| format!("{e}: in a browser the engine opens a share link's set (`url`)"))?;
    if !matches!(web_host::hosted(), HostedState::Idle) {
        return Err("the page's engine has a set open already: one set a page".to_owned());
    }
    begin(WebSource::Link {
        url: named.to_owned(),
    });
    Ok(json!({ "opening": named }))
}

fn begin(source: WebSource) {
    OPENING.with(|o| o.set((page_ms(), None)));
    web_host::open_hosted(source, None);
}

thread_local! {
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
    /// When the engine was first asked to play, and when its set began to
    /// open and its first song was up (the page's ms): reported on the
    /// page's root element ([`report`]).
    static PLAY_ASKED: Cell<Option<f64>> = const { Cell::new(None) };
    static OPENING: Cell<(Option<f64>, Option<f64>)> = const { Cell::new((None, None)) };
}

/// Give the app the engine as its clock, and on a share link open the
/// link's set behind the page. Once per page.
fn install() {
    if INSTALLED.with(|i| i.replace(true)) {
        return;
    }
    let engine: architect_playhead::engine::Shared = Rc::new(RefCell::new(Engine::default()));
    architect_playhead::engine::provide(engine);
    // A song picked in the engine's own panels is the deck's item: the
    // app moves there, and the engine follows as for any press.
    web_host::on_hosted_pick(|project| {
        let Some(note) = web_engine::note_of_project(project) else {
            session_engine::open::switch_song(project);
            web_host::hosted_show(project);
            return;
        };
        let _ = architect_playhead::ask(
            &architect_playhead::Caller::App,
            "PLAYHEAD_ITEM",
            &json!({ "id": note }),
        );
    });
    if let Some(url) = share_link() {
        begin(WebSource::Link { url });
    }
    wasm_bindgen_futures::spawn_local(watch());
}

/// The link this page is on, when it is one (`/s/<code>/…`).
fn share_link() -> Option<String> {
    let location = web_sys::window()?.location();
    let path = location.pathname().ok()?;
    let mut segments = path.trim_start_matches('/').split('/');
    match (segments.next(), segments.next()) {
        (Some("s"), Some(code)) if !code.is_empty() && !code.starts_with('_') => {
            location.href().ok()
        }
        _ => None,
    }
}

/// Which of `songs` (a song's folder in the vault, and its project) the
/// item whose media is at vault path `media` belongs to: the song whose
/// folder holds it, the deepest when folders nest.
#[must_use]
pub fn song_of<'a>(songs: &'a [(String, String)], media: &str) -> Option<&'a str> {
    songs
        .iter()
        .filter(|(folder, _)| {
            folder.is_empty()
                || media
                    .strip_prefix(folder.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        })
        .max_by_key(|(folder, _)| folder.len())
        .map(|(_, song)| song.as_str())
}

/// The project an item's media belongs to, once the engine has it open.
fn project_of(media: &str) -> Option<String> {
    let songs = web_engine::link_songs();
    song_of(&songs, media).map(str::to_owned)
}

fn reading() -> (f64, bool) {
    Transport::shared().map_or((0.0, false), Transport::read)
}

/// The engine as the playhead's clock: which project it was last told to
/// play.
#[derive(Default)]
pub struct Engine {
    project: Option<String>,
}

impl Clock for Engine {
    fn kind(&self) -> &'static str {
        "engine"
    }

    fn load(&mut self, media: Option<&str>) {
        let Some(project) = media.and_then(project_of) else {
            // Nothing of the engine's: it stops where it is.
            if self.project.take().is_some() && reading().1 {
                session_engine::engine::transport(Move::Stop, 0.0);
            }
            return;
        };
        if session_engine::open::current_song().as_deref() != Some(project.as_str()) {
            if reading().1 {
                session_engine::engine::transport(Move::Stop, 0.0);
            }
            session_engine::open::switch_song(&project);
            web_host::hosted_show(&project);
        }
        self.project = Some(project);
    }

    fn play(&mut self) {
        if self.project.is_some() && !reading().1 {
            PLAY_ASKED.with(|p| {
                if p.get().is_none() {
                    p.set(page_ms());
                }
            });
            // The press that reached here is a gesture: the page's audio
            // may start now, if it has not.
            session_ui::web_audio::unlock();
            session_engine::engine::transport(Move::PlayStop, 0.0);
        }
    }

    fn pause(&mut self) {
        if self.project.is_some() && reading().1 {
            session_engine::engine::transport(Move::Stop, 0.0);
        }
    }

    fn seek(&mut self, t: f64) {
        if self.project.is_some() && t.is_finite() {
            session_engine::engine::transport(Move::Seek, t.max(0.0));
        }
    }

    fn read(&self) -> Option<Reading> {
        let project = self.project.as_deref()?;
        let (time, moving) = reading();
        let duration = web_host::hosted_songs()
            .songs
            .iter()
            .find(|song| song.project == project)
            .map(|song| song.span.1)
            .filter(|end| end.is_finite() && *end > 0.0);
        Some(Reading {
            time,
            duration,
            moving,
            ended: duration.is_some_and(|d| time >= d),
            // Silent or not, the engine's time is the song's.
            failed: false,
            resolved: true,
        })
    }
}

impl architect_playhead::EngineClock for Engine {
    fn carries(&self, media: &str) -> bool {
        web_host::hosted_is_open() && project_of(media).is_some()
    }
}

fn page_ms() -> Option<f64> {
    Some(web_sys::window()?.performance()?.now())
}

/// Keeps the deck and the engine agreeing: the engine opening (or another
/// song arriving) hands the item playing to it, and a play or stop made in
/// the engine's own panels becomes the deck's.
#[expect(clippy::future_not_send, reason = "the page has one thread")]
async fn watch() {
    let mut carried = 0usize;
    let mut was_open = false;
    loop {
        gloo_timers::future::TimeoutFuture::new(150).await;
        let now_open = web_host::hosted_is_open();
        let songs = web_engine::link_songs().len();
        if now_open && (!was_open || songs != carried) {
            architect_playhead::engine::changed();
        }
        was_open = now_open;
        carried = songs;
        report(now_open, songs);
        if !now_open || !architect_playhead::engine::is_clock() {
            continue;
        }
        let deck_playing = architect_playhead::with(|d| {
            d.active()
                .map(str::to_owned)
                .and_then(|id| d.source(&id).map(|s| s.transport.playing))
        });
        let (_, engine_playing) = reading();
        if let Some(deck_playing) = deck_playing
            && deck_playing != engine_playing
            && !architect_playhead::engine::settling()
        {
            let action = if engine_playing {
                "PLAYHEAD_PLAY"
            } else {
                "PLAYHEAD_PAUSE"
            };
            let _ = architect_playhead::ask(&architect_playhead::Caller::App, action, &json!({}));
        }
    }
}

/// What the engine is doing, on the page's root element, for whoever
/// checks the page from outside (a browser test reads it, as it reads the
/// app's `<audio data-clock>`): whether it is open, how its song is heard,
/// how many songs it carries, what its worklet has put out, and the times
/// a first sound is made of.
fn report(now_open: bool, songs: usize) {
    let Some(root) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element())
    else {
        return;
    };
    let state = match web_host::hosted() {
        HostedState::Idle => "idle",
        HostedState::Opening(_) => "opening",
        HostedState::Open(_) => "open",
        HostedState::Failed(_) => "failed",
    };
    let (began, up) = OPENING.with(Cell::get);
    if now_open && up.is_none() {
        OPENING.with(|o| o.set((began, page_ms())));
    }
    let probe = session_ui::web_audio::probe();
    let ms = |t: Option<f64>| t.map_or_else(String::new, |t| format!("{t:.0}"));
    let set = |name: &str, value: &str| {
        let _ = root.set_attribute(name, value);
    };
    set("data-engine", state);
    set("data-engine-songs", &songs.to_string());
    set(
        "data-engine-clock",
        if architect_playhead::engine::is_clock() {
            "engine"
        } else {
            "app"
        },
    );
    if now_open {
        set(
            "data-engine-listening",
            &format!("{:?}", web_engine::listening()),
        );
        set("data-engine-time", &format!("{:.2}", reading().0));
    }
    set("data-engine-loud-frames", &probe.loud_frames.to_string());
    set("data-engine-dry-frames", &probe.dry_frames.to_string());
    set("data-engine-peak", &format!("{:.4}", probe.peak_ever));
    set("data-engine-open-began-ms", &ms(began));
    set("data-engine-open-ms", &ms(OPENING.with(Cell::get).1));
    set("data-engine-play-asked-ms", &ms(PLAY_ASKED.with(Cell::get)));
    set("data-engine-first-audio-ms", &ms(probe.first_loud_ms));
}
