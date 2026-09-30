//! Bloom: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::reverb_face("85-verb-bloom", kit::stub(kit::VERB, "Bloom", "#ffb3d9"))
}
