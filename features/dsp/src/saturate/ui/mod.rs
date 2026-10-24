//! FTS Saturate's editor.
//!
//! Five circuits on a rail — Tube, Tape, Transformer, Transistor, Digital —
//! each with its own panel and its own drawing of what it does to a waveform.
//! What a circuit *is* lives in `saturate-profiles`; what it looks like lives
//! here.

pub mod faces;

#[cfg(feature = "saturate-viz")]
pub mod viz;

#[cfg(feature = "saturate-native")]
pub mod control_view;
#[cfg(feature = "saturate-native")]
pub mod emphasis_view;
#[cfg(feature = "saturate-native")]
pub use plugin_core::editor::param_adapter;
#[cfg(feature = "saturate-native")]
pub mod params;
