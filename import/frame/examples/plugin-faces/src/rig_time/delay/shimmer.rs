//! Shimmer: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::delay_face("64-dly-shimmer", kit::stub(kit::DLY, "Shimmer", "#b9d7ff"))
}
