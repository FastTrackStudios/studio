//! The engine side of the binary that links the DAW in (Studio Architect,
//! Studio Desktop): it is its own headless engine
//! (`<binary> --session-engine`, what `session_engine::studio_engine::link`
//! spawns when `FTS_STUDIO_BIN` names no `studio` CLI), and it links its
//! engine as it launches, so the setlist's clock and Lighting follow that
//! engine and no other (`STUDIO_ENGINE_SOCKET`).
//!
//! The engine is another process, in a process group of its own: a
//! relaunched or hot-reloaded window attaches to it again
//! (docs/design/one-engine.md, docs/design/studio-in-architect.md).

use std::time::Duration;

use session_engine::studio_engine::link;

/// `<binary> --session-engine …`: run the headless engine and say how it
/// ended; `None` when the arguments do not ask for it.
#[must_use]
pub fn run_if_asked() -> Option<std::process::ExitCode> {
    let at = std::env::args().position(|a| a == "--session-engine")?;
    architect::host::boot("info");
    let rest: Vec<String> = std::env::args().skip(at + 1).collect();
    let args = session_desktop::engine_host::Args::parse(&rest);
    Some(match session_desktop::engine_host::run(&args) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "the session engine stopped");
            std::process::ExitCode::FAILURE
        }
    })
}

/// Have the Studio engine as the window launches (`FTS_ENGINE`:
/// `spawn`, the default, attaches to a running one or spawns one;
/// `attach` waits for one; `off` none until the DAW is entered), and say
/// its socket in `STUDIO_ENGINE_SOCKET` for the setlist's clock and
/// Lighting. Call it first thing in `main`, before the app starts threads
/// of its own. Its socket, or why there is none.
///
/// # Errors
///
/// None could be had in `wait` (the DAW view asks again when entered).
pub fn link_at_launch(wait: Duration) -> Result<std::path::PathBuf, String> {
    if std::env::var("FTS_ENGINE").is_ok_and(|v| v.trim().eq_ignore_ascii_case("off")) {
        return Err("FTS_ENGINE=off".to_owned());
    }
    if let Some(given) = std::env::var_os("STUDIO_ENGINE_SOCKET").filter(|s| !s.is_empty()) {
        return Ok(given.into());
    }
    let found = link::choose(link::Mode::from_env(), wait)?;
    // SAFETY: called from `main` before the app starts its threads; the
    // only other thread is the spawned engine's reaper, which waits on
    // the child and never reads the environment.
    unsafe { std::env::set_var("STUDIO_ENGINE_SOCKET", &found.socket) };
    Ok(found.socket)
}
