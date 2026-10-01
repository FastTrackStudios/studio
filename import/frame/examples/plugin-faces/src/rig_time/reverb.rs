//! The reverb algorithms' faces, in the block's `algorithm` order.

mod room;
mod hall;
mod plate;
mod spring;
mod cloud;
mod bloom;
mod shimmer;
mod chorale;
mod magneto;
mod nonlinear;
mod swell;
mod reflections;
mod velvet;
mod freeverb;
mod convolution;

use crate::face::Face;

/// Every reverb face, in `kit::VERB_ALGOS` order.
#[must_use]
/// The phone layouts there are, by algorithm (its index in
/// `VERB_ALGOS`'s order, as [`faces`] lists them).
pub fn phone_faces(dark: bool) -> Vec<(usize, Face)> {
    vec![(2, plate::face_at(dark, super::kit::Tier::Phone)), (4, cloud::face_at(dark, super::kit::Tier::Phone))]
}

/// The backs there are (the advanced controls), by algorithm, at `t`.
pub fn back_faces(dark: bool, t: super::kit::Tier) -> Vec<(usize, Face)> {
    vec![(2, plate::back_at(dark, t)), (4, cloud::back_at(dark, t))]
}

pub fn faces(dark: bool) -> Vec<Face> {
    vec![room::face(dark), hall::face(dark), plate::face(dark), spring::face(dark), cloud::face(dark), bloom::face(dark), shimmer::face(dark), chorale::face(dark), magneto::face(dark), nonlinear::face(dark), swell::face(dark), reflections::face(dark), velvet::face(dark), freeverb::face(dark), convolution::face(dark)]
}
