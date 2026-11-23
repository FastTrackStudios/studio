//! FTS Reverb's editor.
//!
//! Seven families on a rail — IR, Hall, Plate, Room, Spring, Ambient,
//! Special — each with its own panel and its own live picture of the space.
//! What a family *is* lives in `reverb-profiles`; what it looks like lives
//! here.

pub mod faces;

/// The live picture of what the effect is doing — one widget, two painters:
/// WGSL where the renderer hands over a device, vectors where it does not.
/// Behind a feature because it wants a Blitz host for the custom-widget
/// mechanism, which a plain wasm remote has not got.
#[cfg(feature = "reverb-viz")]
pub mod viz;

#[cfg(feature = "reverb-native")]
pub mod control_view;
#[cfg(feature = "reverb-native")]
pub mod eq_view;
#[cfg(feature = "reverb-native")]
pub use plugin_core::editor::param_adapter;
#[cfg(feature = "reverb-native")]
pub mod params;
#[cfg(feature = "reverb-native")]
pub mod preset_view;
