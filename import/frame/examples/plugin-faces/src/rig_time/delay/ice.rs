//! Ice: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::delay_face(kit::file("66-dly-ice", dark), kit::stub(kit::DLY, "Ice", "#9fe7ff", dark))
}
