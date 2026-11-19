//! `studio-architect`: Studio's distribution of Architect. With `gui` (the
//! default) the Architect app's window; without it, the headless node
//! (`studio-architect serve`) a hosted Architect runs. Built for iOS, the
//! iPhone and iPad app (`architect_mobile::run`, docs/studio-ios.md).

fn main() -> std::process::ExitCode {
    // The linked arrangement and mixer are one Studio window on Studio's
    // engine: attached once, for both.
    #[cfg(feature = "gui")]
    {
        session_engine::actions::in_host(true);
        studio_arrangement::attach();
    }
    #[cfg(not(target_os = "ios"))]
    return architect_desktop::run(studio_architect::distribution());
    #[cfg(target_os = "ios")]
    return architect_mobile::run(studio_architect::distribution());
}
