//! Spectral: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::delay_face(kit::file("71-dly-spectral", dark), kit::stub(kit::DLY, "Spectral", "#ff9ff3", dark))
}
