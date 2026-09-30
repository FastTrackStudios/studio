//! Chorale: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::reverb_face(kit::file("87-verb-chorale", dark), kit::stub(kit::VERB, "Chorale", "#ffd98a", dark))
}
