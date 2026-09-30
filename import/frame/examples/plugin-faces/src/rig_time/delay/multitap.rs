//! MultiTap: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::delay_face("70-dly-multitap", kit::stub(kit::DLY, "MultiTap", "#8ab4ff"))
}
