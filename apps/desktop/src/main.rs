//! `studio`: Studio's distribution of Architect. With `gui` (the
//! default) the Architect app's window; without it, the headless node
//! (`studio serve`) a hosted Architect runs.

fn main() -> std::process::ExitCode {
    let first = std::env::args().nth(1);
    // What a packaged build's smoke test asks: the release version, with
    // no window, no engine and no display.
    if first.as_deref() == Some("--version") {
        println!("studio {}", fts_version::release_version());
        return std::process::ExitCode::SUCCESS;
    }
    // The linked arrangement and mixer are one Studio window on Studio's
    // engine: attached once, for both. Only where a window opens (no
    // subcommand, or `dev`, a development instance's window): any other
    // subcommand (`serve`, `actions`, `distribution`…) is a node or a
    // client and attaches to no engine.
    #[cfg(feature = "gui")]
    if first
        .as_deref()
        .is_none_or(|a| a == "dev" || (a.starts_with('-') && a != "--headless"))
    {
        session_engine::actions::in_host(true);
        studio_arrangement::attach();
    }
    architect_desktop::run(studio_desktop::distribution())
}
