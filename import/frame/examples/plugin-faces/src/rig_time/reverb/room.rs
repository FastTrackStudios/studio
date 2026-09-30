//! Room: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::reverb_face(kit::file("80-verb-room", dark), kit::stub(kit::VERB, "Room", "#8fd3ff", dark))
}
