//! Swell: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::reverb_face("90-verb-swell", kit::stub(kit::VERB, "Swell", "#c3a6ff"))
}
