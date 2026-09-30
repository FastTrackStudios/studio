//! Convolution: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::reverb_face(kit::file("94-verb-convolution", dark), kit::stub(kit::VERB, "Convolution", "#ffc38a", dark))
}
