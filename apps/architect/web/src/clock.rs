//! The engine as the playhead's clock ([`architect_web::EngineClock`]).
//!
//! The deck knows a song by its item's media, a vault path; the engine
//! knows it by its project. A link's song note and its files sit in one
//! folder, so an item is the song whose folder holds its media
//! ([`crate::song_of`]). An item of a song the engine has open is the
//! engine's: loading it makes that project current, and play, pause and
//! seek move the engine's transport. The deck's time is the engine's
//! (`Reading::time` while it moves), so the chart, the lyrics and the
//! lighting follow what is heard.
//!
//! A press in the engine's own panels (the arrangement's Space, a
//! transport button of the mixer) moves the engine first; [`watch`] hands
//! it to the deck as the app's own press, so the two never disagree about
//! whether the song is playing.

use std::cell::RefCell;
use std::rc::Rc;

use architect_playhead::{Clock, Reading};
use session_engine::engine::{Move, Transport};
use session_engine::web_engine::{self, WebSource};
use session_ui::web_host::HostedState;

/// The engine's clock: which project it was last told to play.
#[derive(Default)]
struct Engine {
    project: Option<String>,
}

fn open() -> bool {
    session_ui::web_host::hosted_is_open()
}

/// The project that is the song an item's media belongs to, once the
/// engine has it open.
fn project_of(media: &str) -> Option<String> {
    let songs = web_engine::link_songs();
    crate::song_of(&songs, media).map(str::to_owned)
}

fn reading() -> (f64, bool) {
    Transport::shared().map_or((0.0, false), Transport::read)
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
            let was_playing = reading().1;
            if was_playing {
                session_engine::engine::transport(Move::Stop, 0.0);
            }
            session_engine::open::switch_song(&project);
            session_ui::web_host::hosted_show(&project);
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
        let duration = session_ui::web_host::hosted_songs()
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

impl architect_web::EngineClock for Engine {
    fn carries(&self, media: &str) -> bool {
        open() && project_of(media).is_some()
    }
}

thread_local! {
    /// When the engine was first asked to play, in the page's
    /// `performance.now()` milliseconds: with the worklet's first frame
    /// that is not silence (`web_audio::probe`), the time to first audio.
    static PLAY_ASKED: std::cell::Cell<Option<f64>> = const { std::cell::Cell::new(None) };
    /// When the link's set began to open, and when its first song was up.
    static OPENING: std::cell::Cell<(Option<f64>, Option<f64>)> =
        const { std::cell::Cell::new((None, None)) };
}

fn page_ms() -> Option<f64> {
    Some(web_sys::window()?.performance()?.now())
}

/// What the engine is doing, on the page's root element, for whoever
/// checks the page from outside (the stack's smoke test reads it, as it
/// reads the app's `<audio data-clock>`): whether it is open, how its
/// song is heard, how many songs it carries, what its worklet has put
/// out (frames that were not silence, underrun frames, the loudest
/// sample), and the three times a first sound is made of.
fn report(now_open: bool, songs: usize) {
    let Some(root) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element())
    else {
        return;
    };
    let state = match &session_ui::web_host::hosted() {
        HostedState::Idle => "idle",
        HostedState::Opening(_) => "opening",
        HostedState::Open(_) => "open",
        HostedState::Failed(_) => "failed",
    };
    let (began, up) = OPENING.with(std::cell::Cell::get);
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
        if architect_web::provided::engine_is_clock() {
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
    set(
        "data-engine-open-ms",
        &ms(OPENING.with(std::cell::Cell::get).1),
    );
    set(
        "data-engine-play-asked-ms",
        &ms(PLAY_ASKED.with(std::cell::Cell::get)),
    );
    set("data-engine-first-audio-ms", &ms(probe.first_loud_ms));
}

/// The link this page is on, when it is one (`/s/<code>/…`).
fn link() -> Option<String> {
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

/// Give the app the engine as its clock, and on a share link open the
/// link's set into it behind the page.
pub fn install() {
    let engine: Rc<RefCell<dyn architect_web::EngineClock>> =
        Rc::new(RefCell::new(Engine::default()));
    architect_web::provide_engine(engine);
    // A song picked in the engine's own panels is the deck's item: the
    // app moves there, and the engine follows as for any press.
    session_ui::web_host::on_hosted_pick(|project| {
        let Some(note) = web_engine::note_of_project(project) else {
            return;
        };
        let _ = architect_playhead::ask(
            &architect_playhead::deck::Caller::App,
            "PLAYHEAD_ITEM",
            &serde_json::json!({ "id": note }),
        );
    });
    let Some(url) = link() else { return };
    OPENING.with(|o| o.set((page_ms(), None)));
    session_ui::web_host::open_hosted(WebSource::Link { url }, None);
    wasm_bindgen_futures::spawn_local(watch());
}

/// Keeps the deck and the engine agreeing: the engine opening (or another
/// song arriving) hands the item playing to it, and a play or stop made in
/// the engine's own panels becomes the deck's.
async fn watch() {
    let mut carried = 0usize;
    let mut was_open = false;
    loop {
        gloo_timers::future::TimeoutFuture::new(150).await;
        let now_open = open();
        let songs = web_engine::link_songs().len();
        if now_open && (!was_open || songs != carried) {
            // What the engine carries changed: the app asks again whose
            // the item is, and the engine takes over where the deck is.
            architect_web::provided::engine_changed();
        }
        was_open = now_open;
        carried = songs;
        report(now_open, songs);
        if !now_open || !architect_web::provided::engine_is_clock() {
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
            && !architect_web::provided::engine_settling()
        {
            let action = if engine_playing {
                "PLAYHEAD_PLAY"
            } else {
                "PLAYHEAD_PAUSE"
            };
            let _ = architect_playhead::ask(
                &architect_playhead::deck::Caller::App,
                action,
                &serde_json::json!({}),
            );
        }
    }
}
