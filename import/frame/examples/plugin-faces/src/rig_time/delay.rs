//! The delay machines' faces, in the block's `style` order.

mod tape;
mod digital;
mod dbucket;
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
pub fn faces() -> Vec<Face> {
    vec![tape::face(), digital::face(), dbucket::face(), lo_fi::face(), shimmer::face(), reverse::face(), ice::face(), rhythm::face(), drum::face(), oil_can::face(), multitap::face(), spectral::face(), filter::face()]
}
