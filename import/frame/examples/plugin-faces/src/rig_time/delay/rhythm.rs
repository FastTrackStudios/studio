//! Rhythm: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::delay_face(kit::file("67-dly-rhythm", dark), kit::stub(kit::DLY, "Rhythm", "#ffb347", dark))
}
