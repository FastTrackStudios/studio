//! `studio-launcher`: Studio's project launcher, served to the host that
//! started it. A browser links the launcher in and has no process copy.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), architect_extensions::guest::GuestError> {
    architect_extensions::guest::Guest::extension(&studio_launcher::Launcher).serve()
}

#[cfg(target_arch = "wasm32")]
fn main() {}
