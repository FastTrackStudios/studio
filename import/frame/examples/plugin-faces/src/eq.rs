//! Faces: the hardware EQs — the passive program equalizer, the console
//! channel EQ, the three-band proportional-Q module and the class-A
//! console channel.
//!
//! Laid out from the units (`eq-ui/src/faces/units.rs` in FTS): what makes
//! each one *that* EQ is its controls — separate boost and cut on one band,
//! swept bands with fixed shapes, coarse stepped frequencies, a stepped
//! mid on a concentric pair — so those are what the faces keep, with the
//! printed scales and colour coding that let an engineer find a band
//! without reading.

mod api;
mod neve;
mod pultec;
mod ssl;

use crate::face::Face;

pub fn faces() -> Vec<Face> {
    vec![pultec::face(), ssl::face(), api::face(), neve::face()]
}
