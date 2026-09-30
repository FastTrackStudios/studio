//! Bloom: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::reverb_face(kit::file("85-verb-bloom", dark), kit::stub(kit::VERB, "Bloom", "#ffb3d9", dark))
}
