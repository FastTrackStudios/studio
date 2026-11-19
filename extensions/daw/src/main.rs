//! `daw-extension`: Studio's engine, served to the host that started it.
//!
//! What the host runs is the launcher (`owner::launch`): it attaches the
//! running engine to the host, starting one first when none runs. The
//! engine itself is `--engine`, in a session of its own, outliving every
//! window (`owner`'s module docs: how it is found, why never two, how it
//! stops). `--stop` stops it, `--status` says whether one runs.

fn main() -> std::process::ExitCode {
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
