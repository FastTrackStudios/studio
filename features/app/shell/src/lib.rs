//! What every FTS app shares around its UI — the pieces Session and Signal
//! each carried a copy of.
//!
//! - [`log_ring`]: every `tracing` event and panic into a bounded ring the
//!   app's Logs view shows, a crash log a panic's trace survives in, and
//!   the flight log a run that ends without warning leaves behind.
//! - `updates` (desktop only): the update check and signed install
//!   against the studio repo's GitHub Releases, on `architect::release`.
//! - `ios_scene` (iOS only): the UIScene lifecycle iOS 27 requires, with
//!   the one scene delegate every app's `Info.plist` names.
//! - `ui` (feature `ui`): the shared studio shell — the frame with its
//!   sidebars and drawers, its saved layout, and the panel registry.

#[cfg(target_os = "ios")]
pub mod ios_scene;
pub mod log_ring;
/// The studio shell's tokens: colours, sizes and the font, shared by the
/// desktop frame (`ui`) and the iPad rig frame (`signal-rig-shell`).
pub mod tokens;
#[cfg(feature = "ui")]
pub mod ui;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod updates;
