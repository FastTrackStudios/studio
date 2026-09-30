//! Magneto: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::reverb_face(kit::file("88-verb-magneto", dark), kit::stub(kit::VERB, "Magneto", "#e0a060", dark))
}
