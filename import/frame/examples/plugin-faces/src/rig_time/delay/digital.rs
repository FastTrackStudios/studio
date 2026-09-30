//! Digital: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::delay_face("61-dly-digital", kit::stub(kit::DLY, "Digital", "#5ff2d2"))
}
