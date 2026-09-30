//! Drum: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::delay_face(kit::file("68-dly-drum", dark), kit::stub(kit::DLY, "Drum", "#ff6b6b", dark))
}
