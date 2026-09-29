//! Faces: the VCA and hybrid compressors — the console bus compressor, the
//! desktop VCA with three knobs, and the digitally-controlled hybrid.
//!
//! Laid out from the units (`comp-ui/src/faces/units.rs` in FTS): the
//! controls each really has, their stepped positions and printed scales,
//! and the meter each reads — a VU movement for the bus compressor, a
//! decibel card for the desktop unit, an LED ladder for the hybrid.

mod dbx160;
mod distressor;
mod ssl_bus;
pub(crate) mod ui;

use crate::face::Face;

pub fn faces() -> Vec<Face> {
    vec![ssl_bus::face(), dbx160::face(), distressor::face()]
}
