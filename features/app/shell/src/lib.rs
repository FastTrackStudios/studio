//! What every FTS app shares around its UI — the pieces Session and Signal
//! each carried a copy of.
//!
//! - [`log_ring`]: every `tracing` event and panic into a bounded ring the
//!   app's Logs view shows, a crash log a panic's trace survives in, and
//!   the flight log a run that ends without warning leaves behind.
//! - [`updates`]: the update check against the studio repo's GitHub
//!   releases.
//! - `ui` (feature `ui`): the shared studio shell — the frame with its
//!   sidebars and drawers, its saved layout, and the panel registry.

pub mod log_ring;
#[cfg(feature = "ui")]
pub mod ui;
pub mod updates;
