//! The browser: the engine is in the page.
//!
//! There is no process to attach to. The `daw` extension's browser build
//! opens the page's one engine (`session_engine::web_engine`, hosted by
//! `session_ui::web_host`: a share link's set, or a member's set from
//! their org) and every view reads the song it has current from there,
//! the same `StudioSession` the desktop's window protocol hands a native
//! view. Actions run on the page's own Action List
//! (`session_engine::web_actions`).

use dioxus::prelude::*;
use serde_json::Value as Json;
use session_engine_client::engine::Move;
use session_ui::song_model::StudioSession;
use session_ui::web_host::{self, HostedState};

/// Nothing to attach to: the page's engine is the `daw` extension's to
/// open. Kept so a view's code is the same on every target.
pub const fn attach() {}

/// Why there is no song yet: the engine opening, failed, or not asked
/// for anything.
#[must_use]
pub fn problem() -> Option<String> {
    match web_host::hosted() {
        HostedState::Open(_) => None,
        HostedState::Failed(why) => Some(why),
        HostedState::Opening(_) => Some("Opening the song in Studio's engine…".to_owned()),
        HostedState::Idle => {
            Some("Studio's engine has nothing open: open a setlist or a song.".to_owned())
        }
    }
}

/// The song the page's engine has current, with what names it (another
/// song mounts a view again).
#[must_use]
pub fn current() -> Option<(String, StudioSession)> {
    if !web_host::hosted_is_open() {
        return None;
    }
    let set = web_host::hosted_songs();
    let song = set.current()?;
    Some((format!("web:{}", song.project), song.session.clone()))
}

/// The song, as a signal a view renders from: looked at a few times a
/// second, changed only when another song (or another set) is current.
pub fn use_song() -> Signal<Option<(String, StudioSession)>> {
    let mut song = use_signal(current);
    use_future(move || async move {
        let mut seen = web_host::hosted_revision();
        loop {
            gloo_timers::future::TimeoutFuture::new(150).await;
            let now = web_host::hosted_revision();
            if now == seen {
                continue;
            }
            seen = now;
            let next = current();
            if next.as_ref().map(|(k, _)| k) != song.peek().as_ref().map(|(k, _)| k) {
                song.set(next);
            }
        }
    });
    song
}

/// The window's mode: Live (a page has no shell to say another).
#[must_use]
pub const fn mode_from_env() -> session::modes::Mode {
    session::modes::Mode::Live
}

/// An action of the engine's Action List, run on the page's list. The
/// transport's two are the engine's own moves (the page's list carries
/// the edits, not the transport).
///
/// # Errors
///
/// No engine in the page, no such action, or it refused.
#[expect(clippy::future_not_send, reason = "the page has one thread")]
pub async fn call(id: &str, args: &Json) -> Result<Json, String> {
    if !web_host::hosted_is_open() {
        return Err(problem().unwrap_or_else(|| "no engine in the page".to_owned()));
    }
    match id {
        "SESSION_PLAY_PAUSE" => {
            session_ui::web_audio::unlock();
            session_engine_client::engine::transport(Move::PlayStop, 0.0);
            return Ok(serde_json::json!({ "ok": true }));
        }
        "SESSION_STOP" => {
            session_engine_client::engine::transport(Move::Stop, 0.0);
            return Ok(serde_json::json!({ "ok": true }));
        }
        _ => {}
    }
    let out = session_engine::web_actions::run(id, args.to_string().as_bytes()).await?;
    Ok(serde_json::from_slice(&out)
        .unwrap_or_else(|_| Json::String(String::from_utf8_lossy(&out).into_owned())))
}
