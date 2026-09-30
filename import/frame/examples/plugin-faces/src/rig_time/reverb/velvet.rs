//! Velvet: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::reverb_face("92-verb-velvet", kit::stub(kit::VERB, "Velvet", "#e6b3ff"))
}
