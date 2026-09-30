//! Chorale: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::reverb_face("87-verb-chorale", kit::stub(kit::VERB, "Chorale", "#ffd98a"))
}
