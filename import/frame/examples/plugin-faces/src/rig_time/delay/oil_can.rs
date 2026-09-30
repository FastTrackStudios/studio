//! Oil Can: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::delay_face(kit::file("69-dly-oil-can", dark), kit::stub(kit::DLY, "Oil Can", "#c08040", dark))
}
