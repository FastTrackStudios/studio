//! Lo-Fi: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::delay_face(kit::file("63-dly-lo-fi", dark), kit::stub(kit::DLY, "Lo-Fi", "#c9a66b", dark))
}
