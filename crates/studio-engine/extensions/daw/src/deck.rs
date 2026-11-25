//! One transport in the app: Studio's engine leads Architect's playhead
//! deck and follows it.
//!
//! The deck (`architect.playhead`) is the app's transport. Its clock is
//! what every host animation on `Clock::Playhead` reads, its Now Playing
//! bar plays and pauses, and the app's other extensions (a chart, a light
//! show) follow it. The engine is its leader for the song it has current:
//!
//! - **Leads.** The current song is this extension's source on the deck
//!   (`set`: its title and length), and each move of the engine's
//!   transport (a play, a pause, a seek, from any view, key or action) is
//!   told to it (`seek`, then `play` or `pause`). While it plays the
//!   engine re-anchors the deck every [`REANCHOR`], so the deck's
//!   extrapolation on the app's clock cannot drift from the engine's
//!   samples.
//! - **Follows.** The deck's own buttons, or another extension's
//!   `PLAYHEAD_*`, reach the engine through the hook on
//!   `app:playhead.changed` ([`heard`]): it plays, pauses or seeks the
//!   engine (`SESSION_PLAY_FROM`, `SESSION_PLAY_PAUSE`, `SESSION_SEEK`).
//!   The deck's events within [`ECHO`] of the engine's own report are
//!   that report coming back, not someone's move.
//!
//! Across machines the transport is the session document's presence
//! (`lead/play`, session#186): Architect at this pin gives an extension
//! no presence, so that is the next step, not this one.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use architect_extensions::linked::Handle;
use serde_json::{Value as Json, json};

/// The deck's service.
pub const SERVICE: &str = "architect.playhead";

/// How often a playing engine tells the deck where it is.
pub const REANCHOR: Duration = Duration::from_secs(4);

/// How long after its own report the engine takes the deck's events as
/// the echo of it.
pub const ECHO: Duration = Duration::from_millis(600);

/// How far the engine may be from where the deck extrapolates it before
/// it is a seek (a locate, a loop's wrap), seconds.
const JUMP: f64 = 0.25;

/// How often the engine's transport is looked at.
const TICK: Duration = Duration::from_millis(30);

static HANDLE: Mutex<Option<Handle>> = Mutex::new(None);

/// The deck calls the engine has made, for `DAW_STATE`.
pub static SENT: AtomicU64 = AtomicU64::new(0);
/// The deck's moves the engine followed, for `DAW_STATE`.
pub static FOLLOWED: AtomicU64 = AtomicU64::new(0);

/// When the engine last told the deck something, in ms since [`epoch`].
static TOLD: AtomicU64 = AtomicU64::new(0);

fn epoch() -> Instant {
    static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

fn now_ms() -> u64 {
    u64::try_from(epoch().elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The host, once the extension is built.
pub fn set_handle(handle: Handle) {
    *HANDLE.lock().unwrap_or_else(PoisonError::into_inner) = Some(handle);
}

fn call(method: &'static str, args: Json) {
    let handle = HANDLE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let (Some(handle), Some(rt)) = (handle, studio_engine::open::runtime()) else {
        return;
    };
    SENT.fetch_add(1, Ordering::Relaxed);
    rt.spawn(async move {
        if let Err(e) = handle.service(SERVICE, method, args).await {
            tracing::warn!(deck.method = method, error = ?e, "daw: the playhead deck refused");
        }
    });
}

/// The song the engine has current: its project, title and length.
fn current() -> Option<(String, String, f64)> {
    studio_engine::studio_engine::set::peek(|view| {
        let set = view.setlist.as_ref()?;
        let song = set.songs.get(set.at)?;
        Some((
            song.project.clone(),
            song.name.clone(),
            song.session.project.length_secs.max(1.0),
        ))
    })
}

/// The song as this extension's source on the deck.
fn publish(title: &str, length: f64, at: f64, playing: bool) {
    told();
    call(
        "set",
        json!({
            "name": "Studio",
            "queue": [{
                "id": "song",
                "title": title.chars().take(120).collect::<String>(),
                "duration": length,
            }],
            "index": 0,
            "time": at.max(0.0),
            "play": playing,
        }),
    );
}

fn told() {
    TOLD.store(now_ms().max(1), Ordering::Relaxed);
}

/// The engine's transport moved: the deck follows.
fn moved(playing: bool, at: f64) {
    crate::session_doc::publish(playing, at, 1.0);
    told();
    call("seek", json!({ "t": at.max(0.0) }));
    call(if playing { "play" } else { "pause" }, json!({}));
}

/// Watches the engine's transport for as long as the process runs, and
/// tells the deck what it does. On a thread of its own.
pub fn lead() {
    let _ = std::thread::Builder::new()
        .name("studio-deck".to_owned())
        .spawn(|| {
            // The song on the deck, and where the deck was told the
            // engine is: `(at, playing, when)`.
            let mut song: Option<String> = None;
            let mut last: Option<(f64, bool, Instant)> = None;
            // When the song's timing was last compared with its session's.
            let mut timed = Instant::now();
            loop {
                std::thread::sleep(TICK);
                let Some((project, title, length)) = current() else {
                    continue;
                };
                if song.as_deref() == Some(project.as_str()) && timed.elapsed() >= RETIME {
                    timed = Instant::now();
                    retime_if_behind(&project);
                }
                let Some((at, playing)) = studio_engine::engine::playhead_of(&project) else {
                    continue;
                };
                if song.as_deref() != Some(project.as_str()) {
                    // Its session document, opened with the engine's timing
                    // and led from here.
                    // Copied out of the set's lock first: opening the file
                    // waits on the engine's runtime.
                    let opening = studio_engine::studio_engine::set::peek(|view| {
                        let set = view.setlist.as_ref()?;
                        let current = set.songs.get(set.at)?;
                        Some((current.saved.clone(), current.session.project.clone()))
                    });
                    if let Some((saved, model)) = opening {
                        let source =
                            saved.or_else(|| crate::opened().map(std::path::PathBuf::from));
                        crate::session_doc::song(&project, source.as_deref(), &model);
                    }
                    publish(&title, length, at, playing);
                    crate::session_doc::publish(playing, at, 1.0);
                    song = Some(project);
                    last = Some((at, playing, Instant::now()));
                    continue;
                }
                let Some((was_at, was_playing, when)) = last else {
                    continue;
                };
                let expected = if was_playing {
                    was_at + when.elapsed().as_secs_f64()
                } else {
                    was_at
                };
                let jumped = (at - expected).abs() > JUMP;
                let stale = playing && when.elapsed() >= REANCHOR;
                if playing != was_playing || jumped || stale {
                    moved(playing, at);
                    last = Some((at, playing, Instant::now()));
                }
            }
        });
}

/// How often the song's sections, markers and tempo are compared with what
/// its session holds.
const RETIME: Duration = Duration::from_secs(1);

/// The current song's session given its timing again when the song's
/// sections, markers or tempo changed (an edit in the arrangement): the
/// chart and the lighting read the session, so they follow.
fn retime_if_behind(project: &str) {
    let Some(now) = crate::session_doc::timing_now(project) else {
        return;
    };
    if crate::session_doc::behind(project, crate::session_doc::timing_of(&now)) {
        crate::session_doc::retime(project, &now);
    }
}

/// The deck moved (`app:playhead.changed`, this extension's source): the
/// engine follows when it was someone else's move.
pub fn heard(args: &Json) {
    // A hook is given `{event, detail}`: the detail is the source's state.
    let state = args.get("detail").unwrap_or(args);
    let told = TOLD.load(Ordering::Relaxed);
    if told != 0 && now_ms().saturating_sub(told) < u64::try_from(ECHO.as_millis()).unwrap_or(0) {
        return;
    }
    let Some((project, ..)) = current() else {
        return;
    };
    let Some((at, playing)) = studio_engine::engine::playhead_of(&project) else {
        return;
    };
    let want_playing = state.get("playing").and_then(Json::as_bool);
    let want_at = state.get("time").and_then(Json::as_f64);
    // Run in order, in one task: a pause then a seek must land in that
    // order (Studio's play/pause, pausing, goes back to where play began).
    let run = |steps: Vec<(&'static str, Json)>| {
        let Some(rt) = studio_engine::open::runtime() else {
            return;
        };
        FOLLOWED.fetch_add(1, Ordering::Relaxed);
        rt.spawn(async move {
            for (id, args) in steps {
                let out = studio_engine::actions::list()
                    .run_json_async(id, args.to_string().as_bytes())
                    .await;
                if let Err(e) = out {
                    tracing::warn!(action.id = id, error = %e, "daw: following the deck refused");
                    return;
                }
            }
        });
    };
    let seek = |t: f64| ("SESSION_SEEK", json!({ "at": t.max(0.0) }));
    match (want_playing, want_at) {
        (Some(true), Some(t)) if !playing || (t - at).abs() > JUMP => {
            run(vec![("SESSION_PLAY_FROM", json!({ "at": t.max(0.0) }))]);
        }
        (Some(true), None) if !playing => run(vec![("SESSION_PLAY_PAUSE", Json::Null)]),
        // Paused where the deck says, or where the engine was.
        (Some(false), t) if playing => run(vec![
            ("SESSION_PLAY_PAUSE", Json::Null),
            seek(t.unwrap_or(at)),
        ]),
        (_, Some(t)) if (t - at).abs() > JUMP => run(vec![seek(t)]),
        _ => {}
    }
}
