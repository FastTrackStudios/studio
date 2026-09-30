//! Reflections: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::reverb_face(kit::file("91-verb-reflections", dark), kit::stub(kit::VERB, "Reflections", "#7fe0d0", dark))
}
