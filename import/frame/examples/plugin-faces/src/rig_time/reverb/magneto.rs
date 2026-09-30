//! Magneto: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::reverb_face("88-verb-magneto", kit::stub(kit::VERB, "Magneto", "#e0a060"))
}
