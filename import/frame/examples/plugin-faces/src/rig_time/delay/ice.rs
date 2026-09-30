//! Ice: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::delay_face("66-dly-ice", kit::stub(kit::DLY, "Ice", "#9fe7ff"))
}
