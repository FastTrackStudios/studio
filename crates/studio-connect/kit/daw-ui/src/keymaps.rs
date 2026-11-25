//! Where daw-ui's keymap profiles come from: the app's (Studio's are the
//! profiles its `keybinds` compiles in), installed with [`install`]. With
//! none installed, no profiles and empty keymaps.

use std::sync::OnceLock;

use architect_input::KeymapConfig;

/// A profile the app offers: its slug, name and description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileInfo {
    pub slug: String,
    pub name: String,
    pub description: String,
}

/// The app's profiles.
#[derive(Clone, Copy)]
pub struct Profiles {
    /// Every profile, as the app lists them.
    pub list: fn() -> Vec<ProfileInfo>,
    /// One profile's keymap, by slug.
    pub keymap: fn(&str) -> Option<KeymapConfig>,
}

static PROFILES: OnceLock<Profiles> = OnceLock::new();

/// Install the app's profiles: once per process, at start.
pub fn install(profiles: Profiles) {
    let _ = PROFILES.set(profiles);
}

/// Every profile the app offers.
#[must_use]
pub fn list() -> Vec<ProfileInfo> {
    PROFILES.get().map_or_else(Vec::new, |p| (p.list)())
}

/// A profile's keymap, by slug.
#[must_use]
pub fn keymap(slug: &str) -> Option<KeymapConfig> {
    PROFILES.get().and_then(|p| (p.keymap)(slug))
}
