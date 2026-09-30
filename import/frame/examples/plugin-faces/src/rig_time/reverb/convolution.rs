//! Convolution: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::reverb_face("94-verb-convolution", kit::stub(kit::VERB, "Convolution", "#ffc38a"))
}
