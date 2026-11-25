//! Frame on architect: what connects Frame's core (`frame`, which has no
//! architect, vox or Dioxus dependency) to architect apps.
//!
//! - [`engine`]: the `SurfaceEngine` vox service, for every core engine.
//! - [`memory`]: an in-memory mock engine for previews, tests and design.
//! - [`remote`] (feature `remote`): engines over a WebSocket, as a runtime
//!   `Engine`, and the telemetry pump.
//! - [`library`] (feature `library`): shared surface libraries — the
//!   protocol, the filesystem store and the live link.
//! - [`theme`] (feature `theme`): a host's theme on the Action List —
//!   inspect, apply, edit, reload, errors, save, revert, capture.

pub mod engine;
#[cfg(feature = "library")]
pub mod library;
pub mod memory;
#[cfg(feature = "remote")]
pub mod remote;
#[cfg(feature = "theme")]
pub mod theme;

pub use engine::*;
