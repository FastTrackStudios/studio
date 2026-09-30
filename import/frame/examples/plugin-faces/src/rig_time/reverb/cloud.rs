//! Cloud: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::reverb_face("84-verb-cloud", kit::stub(kit::VERB, "Cloud", "#a6c8ff"))
}
