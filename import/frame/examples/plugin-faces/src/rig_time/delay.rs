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
/// The phone layouts there are, by machine (its index in `DELAY_ALGOS`'s
/// order, as [`faces`] lists them).
pub fn phone_faces(dark: bool) -> Vec<(usize, Face)> {
    small_faces(dark, super::kit::Tier::Phone)
}

/// The small-room layouts at `t` (a phone's lane, a desktop pane's), by
/// machine.
pub fn small_faces(dark: bool, t: super::kit::Tier) -> Vec<(usize, Face)> {
    vec![(0, tape::face_at(dark, t)), (1, digital::face_at(dark, t)), (2, analog::face_at(dark, t))]
}

/// The backs there are (the advanced controls), by algorithm, at `t`.
pub fn back_faces(dark: bool, t: super::kit::Tier) -> Vec<(usize, Face)> {
    vec![(0, tape::back_at(dark, t)), (1, digital::back_at(dark, t)), (2, analog::back_at(dark, t))]
}

pub fn faces(dark: bool) -> Vec<Face> {
    vec![tape::face(dark), digital::face(dark), analog::face(dark), lo_fi::face(dark), shimmer::face(dark), reverse::face(dark), ice::face(dark), rhythm::face(dark), drum::face(dark), oil_can::face(dark), multitap::face(dark), spectral::face(dark), filter::face(dark)]
}
