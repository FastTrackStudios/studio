//! FreeVerb: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::reverb_face("93-verb-freeverb", kit::stub(kit::VERB, "FreeVerb", "#b0c4de"))
}
