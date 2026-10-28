//! The engine with no window: what `studio engine` stands up.
//!
//! The same engine a window's process runs — one `Standalone`, its
//! runtime, the daw facade over it, the set's one audio engine (opened with
//! the first song), the Action List on MCP — started without a window to
//! ask for any of it. Everything a window would do to it goes through the
//! Action List ([`super::set_actions`], `crate::actions`) or the facade, so
//! the CLI and a model drive it exactly as they drive a window's.
//!
//! Serving the facade to other processes (a Unix socket, a WebSocket) is
//! the host's: `session_desktop::cli::engine`.

use daw::standalone::Standalone;

/// The engine, up.
pub struct Engine {
    /// The process's engine: every song of the set is a project in it.
    pub daw: Standalone,
    /// The runtime the engine, its facade and the Action List run on.
    pub runtime: &'static tokio::runtime::Runtime,
}

/// Stand the engine up in this process, in Engine mode (it owns its
/// projects and plays them), with the Action List served on MCP
/// (`super::mcp`; `FTS_MCP=0` for none).
///
/// # Errors
///
/// The engine's runtime or its facade could not be started.
pub fn start() -> eyre::Result<Engine> {
    crate::open::set_mode(crate::open::ModeState::engine());
    let daw = crate::open::start_engine()?;
    crate::open::serve_actions();
    let runtime = crate::open::runtime()
        .ok_or_else(|| eyre::eyre!("the engine started without its runtime"))?;
    tracing::info!(
        engine.actions = crate::actions::list().len(),
        "engine: up, with no window"
    );
    Ok(Engine { daw, runtime })
}
