//! The Logs view's side of the flight recorder.
//!
//! The recorder itself is architect's (`architect_telemetry::flight`): a
//! `tracing` layer ([`RingLayer`]) keeps the last lines in a bounded ring,
//! and one panic hook per process ([`install_panic_hook`]) puts a panic in
//! the ring and leaves two files in the app's data folder
//! (`<data dir>/<app>/`):
//!
//! - `crash.log` ([`crash_log`]): each panic, its backtrace and what was
//!   logged just before it, appended;
//! - `flight-log.txt` ([`flight_log`]): the ring itself, written on a
//!   panic and whenever the app calls [`persist`] (the rig's audio watcher
//!   does, every few seconds), so a run the system kills without a panic
//!   still leaves its last lines; [`restore_previous`] puts them on the
//!   Logs view at the next launch.
//!
//! What stays here is the app's part: which folder an app's files go in,
//! and a [`snapshot`] the Logs view can call in the browser too, where
//! there is no recorder and it is empty.

#[cfg(not(target_arch = "wasm32"))]
pub use architect_telemetry::flight::{
    RingLayer, crash_log, flight_log, persist, push, restore_previous, snapshot,
};

/// Install the flight recorder's panic hook for `app` (`Session`,
/// `Signal`), chained onto the hook before it: its files go in
/// `<data dir>/<app>/`.
///
/// Call it once per process, after the subscriber with its [`RingLayer`];
/// a second call installs nothing. `false` when it was already installed
/// or this platform has no data folder.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::must_use_candidate)]
pub fn install_panic_hook(app: &str) -> bool {
    dirs::data_dir()
        .is_some_and(|dir| architect_telemetry::flight::install_panic_recorder(dir.join(app)))
}

/// In the browser nothing records: the Logs view shows nothing.
#[cfg(target_arch = "wasm32")]
#[must_use]
pub fn snapshot() -> Vec<String> {
    Vec::new()
}
