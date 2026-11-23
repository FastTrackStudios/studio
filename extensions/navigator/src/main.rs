//! `studio-navigator`: Studio's progress bar and set navigator, served to
//! the host that started it.

#[cfg(not(target_arch = "wasm32"))]
use architect_extensions_dioxus::guest::GuestExt as _;
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), architect_extensions::guest::GuestError> {
    studio_view_link::attach();
    architect_extensions::guest::Guest::extension(&studio_navigator::Navigator).serve()
}

/// The program is the native process's; in a browser the extension is
/// linked into the web app.
#[cfg(target_arch = "wasm32")]
fn main() {}
