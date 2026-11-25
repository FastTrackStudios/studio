//! What a window asks of the app it is in: whether the window is the app's
//! dock (then a pane key toggles the dock's panel instead), asking the dock
//! for a panel or for the docks, and what mode the app shows.
//!
//! The app installs its answers ([`install`]); with none, the window is no
//! dock and shows no mode.

use std::sync::OnceLock;

/// What a window asks of the app's dock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockAsk {
    /// Open the panel of this kind, or close the ones shown.
    TogglePanel(&'static str),
    /// Hide every edge dock, or bring back the ones hidden.
    ToggleDocks,
}

/// The app's answers.
#[derive(Clone, Copy)]
pub struct App {
    /// Whether the window is the app's dock now.
    pub docked: fn() -> bool,
    /// Ask the app's dock for something.
    pub dock: fn(DockAsk),
    /// The mode the app shows, by name, and its phase's slug, when it has
    /// phases: what `SESSION_WINDOW` answers.
    pub shown: fn() -> (Option<String>, Option<String>),
}

static APP: OnceLock<App> = OnceLock::new();

/// Install the app's answers: once per process, at start.
pub fn install(app: App) {
    let _ = APP.set(app);
}

/// Whether the window is the app's dock.
#[must_use]
pub fn docked() -> bool {
    APP.get().is_some_and(|app| (app.docked)())
}

/// Ask the app's dock for `ask`.
pub fn request_dock(ask: DockAsk) {
    if let Some(app) = APP.get() {
        (app.dock)(ask);
    }
}

/// Ask the dock for `ask` when the window is a dock, saying whether it
/// did — else the caller presses its pane, as before the dock.
pub fn if_docked(ask: DockAsk) -> bool {
    let docked = docked();
    if docked {
        request_dock(ask);
    }
    docked
}

/// The mode the app shows and its phase.
#[must_use]
pub fn shown() -> (Option<String>, Option<String>) {
    APP.get().map_or((None, None), |app| (app.shown)())
}

/// The action that puts the app in a mode (`{"mode": name}`).
pub const MODE_ACTION: &str = "SESSION_MODE";

/// The action that hides or brings back the app's docks.
pub const DOCKS_ACTION: &str = "SESSION_DOCKS";
