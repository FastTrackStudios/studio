//! The app's playhead deck, driven by the engine.
//!
//! The engine leads: its song is this extension's source on the deck
//! (`architect.playhead` `set`), and every move of its transport is told
//! to the deck (`seek`, `play`, `pause`). The deck's clock is what every
//! host animation on `Clock::Playhead` reads, so the arrangement's
//! playhead line moves with no word from any view, and the app's Now
//! Playing bar shows the song. The bar's own buttons come back through a
//! hook on `app:playhead.changed` ([`heard`]) and the engine follows.
//!
//! The deck extrapolates between the engine's reports on the app's clock;
//! the engine re-anchors it every few seconds while it plays
//! ([`REANCHOR`]), so the two clocks can't drift apart visibly.

use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value as Json, json};
use session::Session;

/// The deck's service.
pub const SERVICE: &str = "architect.playhead";

/// How often a playing engine tells the deck where it is.
pub const REANCHOR: std::time::Duration = std::time::Duration::from_secs(4);

/// The deck's reports the engine has sent: a test and `DAW_STATE` read it.
pub static SENT: AtomicU64 = AtomicU64::new(0);

fn call(method: &'static str, args: Json) {
    let (Some(handle), Some(rt)) = (crate::handle(), crate::RUNTIME.get()) else {
        return;
    };
    SENT.fetch_add(1, Ordering::Relaxed);
    rt.spawn(async move {
        if let Err(e) = handle.service(SERVICE, method, args).await {
            let _ = crate::handle().map(|h| {
                h.host(architect_extensions::hostcall::HostRequest::Log {
                    text: format!("daw: the playhead deck refused {method}: {e:?}"),
                    level: Some("warn".to_owned()),
                })
            });
        }
    });
}

/// The song `s` as this extension's source on the deck: its length,
/// sections (the session's regions) and beat.
pub fn publish(s: &Session) {
    let (tracks, _) = crate::import::engine_tracks(s);
    let duration = tracks
        .iter()
        .flat_map(|t| t.track.items.iter().map(|c| c.position + c.length))
        .fold(1.0_f64, f64::max);
    let sections: Vec<Json> = s
        .regions
        .values()
        .filter_map(|r| {
            let (a, b) = (r.start.seconds(&s.tempo), r.end.seconds(&s.tempo));
            (b > a).then(|| {
                json!({
                    "name": r.name.chars().take(40).collect::<String>(),
                    "start": a,
                    "end": b,
                })
            })
        })
        .collect();
    let meter = s.tempo.default_meter().numerator.clamp(1, 16);
    call(
        "set",
        json!({
            "name": "DAW",
            "queue": [{
                "id": "song",
                "title": s.name.chars().take(120).collect::<String>(),
                "duration": duration,
                "sections": sections,
                "tempo": { "bpm": s.tempo.default_bpm().clamp(20.0, 400.0), "at": 0.0, "meter": meter },
            }],
            "index": 0,
            "time": 0.0,
            "play": false,
        }),
    );
}

/// When the engine last told the deck something, in ms of the process's
/// clock: the deck's events that follow are its echo, not someone's move.
static TOLD: AtomicU64 = AtomicU64::new(0);

/// How long after its own report the engine takes the deck's events as
/// the echo of it.
pub const ECHO: std::time::Duration = std::time::Duration::from_millis(600);

fn now_ms() -> u64 {
    static EPOCH: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    u64::try_from(
        EPOCH
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

/// The engine's transport moved: the deck follows.
pub fn moved(playing: bool, position: f64) {
    TOLD.store(now_ms().max(1), Ordering::Relaxed);
    call("seek", json!({ "t": position.max(0.0) }));
    call(if playing { "play" } else { "pause" }, json!({}));
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
    let playing = state.get("playing").and_then(Json::as_bool);
    let time = state.get("time").and_then(Json::as_f64);
    #[cfg(feature = "audio")]
    crate::engine::follow(playing, time);
    let _ = (playing, time);
}
