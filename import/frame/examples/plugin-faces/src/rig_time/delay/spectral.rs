//! Spectral: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::delay_face("71-dly-spectral", kit::stub(kit::DLY, "Spectral", "#ff9ff3"))
}
