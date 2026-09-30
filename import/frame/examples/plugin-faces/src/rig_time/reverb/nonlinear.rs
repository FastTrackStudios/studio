//! NonLinear: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::reverb_face("89-verb-nonlinear", kit::stub(kit::VERB, "NonLinear", "#ff8a8a"))
}
