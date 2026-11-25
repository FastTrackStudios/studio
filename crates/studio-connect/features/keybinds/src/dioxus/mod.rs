//! Studio's Dioxus input views.
//!
//! The which-key overlay (architect-ui styled, tuned for the Blitz
//! overlay in REAPER) and the keymap visualizer. Event conversion and the
//! processor hook are `architect_input::dioxus`.

pub mod focus;
pub mod visualizer;
pub mod which_key;

pub use focus::TEXT_INPUT_FOCUS_COUNT;
pub use visualizer::InputVisualizer;
pub use which_key::{OverlayEntry, WhichKeyOverlay, WhichKeyState};
