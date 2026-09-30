//! Shimmer: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face(dark: bool) -> Face {
    kit::reverb_face(kit::file("86-verb-shimmer", dark), kit::stub(kit::VERB, "Shimmer", "#d9f0ff", dark))
}
