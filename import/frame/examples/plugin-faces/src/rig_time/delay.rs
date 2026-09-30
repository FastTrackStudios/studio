//! The delay machines' faces, in the block's `style` order.

mod tape;
mod digital;
mod analog;
mod lo_fi;
mod shimmer;
mod reverse;
mod ice;
mod rhythm;
mod drum;
mod oil_can;
mod multitap;
mod spectral;
mod filter;

use crate::face::Face;

/// Every delay face, in `kit::DELAY_ALGOS` order.
#[must_use]
pub fn faces(dark: bool) -> Vec<Face> {
    vec![tape::face(dark), digital::face(dark), analog::face(dark), lo_fi::face(dark), shimmer::face(dark), reverse::face(dark), ice::face(dark), rhythm::face(dark), drum::face(dark), oil_can::face(dark), multitap::face(dark), spectral::face(dark), filter::face(dark)]
}
