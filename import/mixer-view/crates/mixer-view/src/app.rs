//! What the mixer asks of the app it is in: the app's modes (the corner's
//! selector), the scenes a mode offers an instrument (the left rail), the
//! phase the app's mix is in (which the mixer's rails follow and set), and
//! how the app names one half of a stereo pair.
//!
//! The app installs its answers ([`install`]); with none, the mixer shows
//! no modes and no scenes, keeps its own phase and pairs nothing.

use std::sync::OnceLock;

/// Who the mixer is laid out for: the engineer's full view, or the
/// player's overview of the same flow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Audience {
    #[default]
    Engineer,
    Player,
}

/// A scene a rail offers: its slug and its short label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scene {
    pub slug: &'static str,
    pub short: &'static str,
}

/// The app's answers.
#[derive(Clone, Copy)]
pub struct App {
    /// The app's modes, in order: each its slug and its name.
    pub modes: fn() -> &'static [(&'static str, &'static str)],
    /// The scenes `mode` offers `instrument`, in rail order.
    pub scenes: fn(mode: &str, instrument: &str) -> Vec<Scene>,
    /// The scene named `slug`, if the app has one.
    pub scene: fn(slug: &str) -> Option<Scene>,
    /// The mix's phase, as the app last chose it (its slug), and a
    /// counter that moves whenever it is chosen.
    pub phase: fn() -> (Option<&'static str>, u64),
    /// Choose the mix's phase (its slug).
    pub choose_phase: fn(&str),
    /// Whether a track is one half of a stereo pair, by its name.
    pub is_pair_half: fn(&str) -> bool,
}

static APP: OnceLock<App> = OnceLock::new();

/// Install the app's answers: once per process, at start.
pub fn install(app: App) {
    let _ = APP.set(app);
}

/// The app's modes.
#[must_use]
pub fn modes() -> &'static [(&'static str, &'static str)] {
    APP.get().map_or(&[], |app| (app.modes)())
}

/// The scenes `mode` offers `instrument`.
#[must_use]
pub fn scenes(mode: &str, instrument: &str) -> Vec<Scene> {
    APP.get()
        .map_or_else(Vec::new, |app| (app.scenes)(mode, instrument))
}

/// The scene `slug`, if the app has one.
#[must_use]
pub fn scene(slug: &str) -> Option<Scene> {
    APP.get().and_then(|app| (app.scene)(slug))
}

/// The mix's phase as the app has it, and its counter.
#[must_use]
pub fn phase() -> (Option<&'static str>, u64) {
    APP.get().map_or((None, 0), |app| (app.phase)())
}

/// Choose the mix's phase.
pub fn choose_phase(slug: &str) {
    if let Some(app) = APP.get() {
        (app.choose_phase)(slug);
    }
}

/// Whether a track is one half of a stereo pair.
#[must_use]
pub fn is_pair_half(name: &str) -> bool {
    APP.get().is_some_and(|app| (app.is_pair_half)(name))
}

/// The mode the mixer's rails are for: mixing.
pub const MIX: &str = "mix";
