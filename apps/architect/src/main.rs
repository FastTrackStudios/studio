//! `studio-architect`: Studio's distribution of Architect. With `gui` (the
//! default) the Architect app's window; without it, the headless node
//! (`studio-architect serve`) a hosted Architect runs.

fn main() -> std::process::ExitCode {
    architect_desktop::run(studio_architect::distribution())
}
