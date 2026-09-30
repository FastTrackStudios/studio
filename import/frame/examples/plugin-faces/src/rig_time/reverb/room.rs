//! Room: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::reverb_face("80-verb-room", kit::stub(kit::VERB, "Room", "#8fd3ff"))
}
