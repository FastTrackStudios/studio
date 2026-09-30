//! Filter: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::delay_face("72-dly-filter", kit::stub(kit::DLY, "Filter", "#ffd166"))
}
