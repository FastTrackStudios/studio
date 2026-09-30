//! FreeVerb: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::reverb_face(kit::file("93-verb-freeverb", dark), kit::stub(kit::VERB, "FreeVerb", "#b0c4de", dark))
}
