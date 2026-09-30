//! Spring: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::reverb_face(kit::file("83-verb-spring", dark), kit::stub(kit::VERB, "Spring", "#9be38a", dark))
}
