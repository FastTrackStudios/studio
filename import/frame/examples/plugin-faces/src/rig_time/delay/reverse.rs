//! Reverse: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::delay_face(kit::file("65-dly-reverse", dark), kit::stub(kit::DLY, "Reverse", "#d98aff", dark))
}
