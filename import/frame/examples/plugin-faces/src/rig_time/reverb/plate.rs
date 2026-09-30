//! Plate: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::reverb_face("82-verb-plate", kit::stub(kit::VERB, "Plate", "#e8e6e1"))
}
