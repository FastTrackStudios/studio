//! Shimmer: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::reverb_face("86-verb-shimmer", kit::stub(kit::VERB, "Shimmer", "#d9f0ff"))
}
