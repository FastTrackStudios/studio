//! Lo-Fi: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::delay_face("63-dly-lo-fi", kit::stub(kit::DLY, "Lo-Fi", "#c9a66b"))
}
