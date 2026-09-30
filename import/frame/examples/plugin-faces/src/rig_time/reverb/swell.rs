//! Swell: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::reverb_face(kit::file("90-verb-swell", dark), kit::stub(kit::VERB, "Swell", "#c3a6ff", dark))
}
