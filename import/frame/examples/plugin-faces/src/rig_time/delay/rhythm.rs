//! Rhythm: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::delay_face("67-dly-rhythm", kit::stub(kit::DLY, "Rhythm", "#ffb347"))
}
