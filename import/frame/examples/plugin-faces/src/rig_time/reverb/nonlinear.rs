//! NonLinear: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::reverb_face(kit::file("89-verb-nonlinear", dark), kit::stub(kit::VERB, "NonLinear", "#ff8a8a", dark))
}
