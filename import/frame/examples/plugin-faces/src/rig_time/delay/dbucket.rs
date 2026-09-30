//! dBucket: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::delay_face("62-dly-dbucket", kit::stub(kit::DLY, "dBucket", "#7fd45a"))
}
