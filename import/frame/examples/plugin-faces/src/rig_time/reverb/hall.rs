//! Hall: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::reverb_face("81-verb-hall", kit::stub(kit::VERB, "Hall", "#b39dff"))
}
