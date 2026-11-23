//! `studio-arrangement`: Studio's arrangement view, served to the host that
//! started it.

#[cfg(not(target_arch = "wasm32"))]
use architect_extensions_dioxus::guest::GuestExt as _;
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), architect_extensions::guest::GuestError> {
    // The mixer's window actions are the mixer pane's (`studio.mixer`
    // claims them): a key here that runs one goes to the engine, which
    // asks that pane.
    session_engine::window_queue::hand_over(&studio_view_link::MIXER_ACTIONS);
    studio_arrangement::attach();
    architect_extensions::guest::Guest::extension(&studio_arrangement::Arrangement).serve()
}

/// The program is the native process's; in a browser the extension is
/// linked into the web app.
#[cfg(target_arch = "wasm32")]
fn main() {}
