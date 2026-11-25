//! Where the editor's key and wheel bindings come from: the app's keymaps
//! (Studio's are its `keybinds` defaults with the user's file over them),
//! installed with [`install`]. With none installed, no bindings.

use std::sync::OnceLock;

use architect_input::KeymapConfig;

/// The app's keymaps: the defaults, and the user's own when there is one.
#[derive(Clone, Copy)]
pub struct Keymaps {
    pub defaults: fn() -> KeymapConfig,
    pub user: fn() -> Option<KeymapConfig>,
}

static KEYMAPS: OnceLock<Keymaps> = OnceLock::new();

/// Install the app's keymaps: once per process, at start.
pub fn install(keymaps: Keymaps) {
    let _ = KEYMAPS.set(keymaps);
}

/// The default bindings.
#[must_use]
pub fn defaults() -> KeymapConfig {
    KEYMAPS
        .get()
        .map_or_else(KeymapConfig::default, |k| (k.defaults)())
}

/// The user's own bindings, when there are any.
#[must_use]
pub fn user() -> Option<KeymapConfig> {
    KEYMAPS.get().and_then(|k| (k.user)())
}
