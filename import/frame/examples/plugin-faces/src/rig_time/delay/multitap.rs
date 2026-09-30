//! MultiTap: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::delay_face(kit::file("70-dly-multitap", dark), kit::stub(kit::DLY, "MultiTap", "#8ab4ff", dark))
}
