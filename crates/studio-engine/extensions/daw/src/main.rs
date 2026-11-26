//! `daw-extension`: Studio's engine, served to the host that started it.
//!
//! What the host runs is the launcher (`owner::launch`): it attaches the
//! running engine to the host, starting one first when none runs. The
//! engine itself is `--engine`, in a session of its own, outliving every
//! window (`owner`'s module docs: how it is found, why never two, how it
//! stops). `--stop` stops it, `--status` says whether one runs.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    // The running version for what the engine reports (its spans, its crash
    // reports, its MCP server): the release's, not the workspace's.
    utils::version::set(fts_version::release_version());
    // A render worker (`--render-worker`, spawned by an engine with its
    // socket in the environment): renders an agent asked for, apart from
    // the person's session.
    if let Some(result) = studio_engine::render_worker::main_if_worker(studio_engine::install) {
        return match result {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                use std::io::Write as _;
                let _ = writeln!(std::io::stderr(), "daw-extension: render worker: {e}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    let arg = std::env::args().nth(1);
    let result = match arg.as_deref() {
        Some("--engine") => daw_extension::owner::engine(),
        Some("--stop") => daw_extension::owner::stop().map(|was| {
            tracing::info!(engine.was_running = was, "daw: stop");
        }),
        Some("--status") => match daw_extension::owner::status() {
            Some(json) => {
                use std::io::Write as _;
                let _ = std::io::stdout().write_all(json.as_bytes());
                Ok(())
            }
            None => Err("no engine runs".to_owned()),
        },
        _ => daw_extension::owner::launch(),
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "daw-extension");
            use std::io::Write as _;
            let _ = writeln!(std::io::stderr(), "daw-extension: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// The program is the native process's; in a browser the extension is
/// linked into the web app.
#[cfg(target_arch = "wasm32")]
fn main() {}
