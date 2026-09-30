//! Tape: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::delay_face("60-dly-tape", kit::stub(kit::DLY, "Tape", "#e0a040"))
}
