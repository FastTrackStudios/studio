//! `studio-architect`: Studio's distribution of Architect. With `gui` (the
//! default) the Architect app's window; without it, the headless node
//! (`studio-architect serve`) a hosted Architect runs.

fn main() -> std::process::ExitCode {
    // The linked arrangement and mixer are one Studio window on Studio's
    // engine: attached once, for both.
    #[cfg(feature = "gui")]
    {
        session_engine::actions::in_host(true);
        studio_arrangement::attach();
    }
    architect_desktop::run(studio_architect::distribution())
}
