//! Faces: the VCA and hybrid compressors — the console bus compressor in
//! its rack form, the desktop VCA with three knobs, and the digitally
//! controlled character compressor.
//!
//! Each is laid out from photographs of the real unit: its proportions, the
//! controls it really has, their stepped positions and printed scales, and
//! the meter it reads — an amber compression movement for the bus
//! compressor, a blue-printed decibel card for the desktop unit, an LED
//! ladder for the character compressor.

mod bus_comp;
mod character_comp;
pub(crate) mod ui;
mod vca_comp;

use crate::face::Face;

pub fn faces() -> Vec<Face> {
    vec![bus_comp::face(), vca_comp::face(), character_comp::face()]
}
