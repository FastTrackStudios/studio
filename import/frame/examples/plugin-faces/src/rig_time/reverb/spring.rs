//! Spring: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::reverb_face("83-verb-spring", kit::stub(kit::VERB, "Spring", "#9be38a"))
}
