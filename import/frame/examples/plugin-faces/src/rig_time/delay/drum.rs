//! Drum: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::delay_face("68-dly-drum", kit::stub(kit::DLY, "Drum", "#ff6b6b"))
}
