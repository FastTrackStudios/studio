//! Reverse: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::delay_face("65-dly-reverse", kit::stub(kit::DLY, "Reverse", "#d98aff"))
}
