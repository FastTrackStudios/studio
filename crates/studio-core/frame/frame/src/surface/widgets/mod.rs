//! The canonical control library.
//!
//! - [`theme`]: one set of semantic tokens (colors, sizes, spacing).
//! - [`widgets`]: knob, fader, button, footswitch, meter, readout — each a
//!   plain design subtree plus a [`Control`](crate::surface::proto::Control) with
//!   its slots wired.
//! - [`templates`]: whole surfaces from an engine [`Catalog`](crate::surface::proto::Catalog).

pub mod templates;
pub mod theme;
pub mod widgets;

pub use templates::{Options, hug, surface_for_catalog, wrap_rows};
pub use theme::{ControlSize, Theme};
pub use widgets::{Builder, sector_path};
