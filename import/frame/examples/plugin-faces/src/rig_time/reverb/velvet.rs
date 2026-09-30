//! Velvet: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::reverb_face(kit::file("92-verb-velvet", dark), kit::stub(kit::VERB, "Velvet", "#e6b3ff", dark))
}
