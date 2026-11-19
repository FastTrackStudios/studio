//! `studio-architect`: Studio's distribution of Architect. With `gui` (the
//! default) the Architect app's window; without it, the headless node
//! (`studio-architect serve`) a hosted Architect runs.
//!
//! `studio-architect dev-source <id>` writes a built-in's crate (one of
//! Studio's, by its `[source]`) from this checkout into the app's
//! development folder, where `EXT_CLONE {id}` builds it and runs it over
//! the built-in under the same id: the vendored source Architect's own app
//! has for its built-ins, which a distribution cannot register yet.

use std::io::Write as _;

fn main() -> std::process::ExitCode {
    // rustls with two backends linked (ring, aws-lc-rs) picks none by
    // itself: ring for the whole process, before anything dials TLS.
    // Refused only when one is installed already, which is as good.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [verb, id] = args.as_slice()
        && verb == "dev-source"
    {
        return match studio_architect::dev_source(id) {
            Ok(folder) => {
                let _ = writeln!(std::io::stdout(), "{}", folder.display());
                std::process::ExitCode::SUCCESS
            }
            Err(error) => {
                let _ = writeln!(std::io::stderr(), "studio-architect dev-source: {error}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    // `studio-architect --session-engine …`: the headless Studio engine the
    // window spawns (the `daw` feature, docs/design/studio-in-architect.md).
    #[cfg(feature = "daw")]
    if let Some(code) = studio_daw::engine::run_if_asked() {
        return code;
    }
    let opens_window = args.first().is_none_or(|a| a.starts_with("--") && a != "--headless");
    // The window's engine, linked as it launches (spawned or attached;
    // `FTS_ENGINE`): its socket in `STUDIO_ENGINE_SOCKET`, so the setlist's
    // clock and Lighting follow this engine. Before any thread of the app.
    #[cfg(feature = "daw")]
    if opens_window {
        if let Err(why) = studio_daw::engine::link_at_launch(std::time::Duration::from_secs(30)) {
            let _ = writeln!(std::io::stderr(), "studio-architect: no engine at launch ({why}); the DAW view asks again");
        }
    }
    // `STUDIO_SERVE=1`: Studio Desktop is the server, its node (vault,
    // sync, share links, the web app's vox) in this process beside the
    // window, configured as `serve` is (ARCHITECT_*).
    if opens_window && std::env::var("STUDIO_SERVE").is_ok_and(|v| v != "0" && !v.is_empty()) {
        studio_architect::serve_beside();
    }
    // Session's shell (the DAW view) awaits Tokio timers and tasks on the
    // UI thread, as Studio's window (`dioxus_native::launch_cfg`, which
    // enters a runtime there) lets it; Architect's launcher enters none. A
    // runtime entered on this, the UI thread, for the window's life.
    #[cfg(feature = "daw")]
    let runtime = opens_window.then(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("studio-ui-rt")
            .build()
    });
    #[cfg(feature = "daw")]
    let _entered = match &runtime {
        Some(Ok(rt)) => Some(rt.enter()),
        Some(Err(error)) => {
            let _ = writeln!(std::io::stderr(), "studio-architect: no runtime for the DAW view ({error})");
            None
        }
        None => None,
    };
    architect_desktop::run(studio_architect::distribution())
}
