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
pub fn faces(dark: bool) -> Vec<Face> {
    vec![room::face(dark), hall::face(dark), plate::face(dark), spring::face(dark), cloud::face(dark), bloom::face(dark), shimmer::face(dark), chorale::face(dark), magneto::face(dark), nonlinear::face(dark), swell::face(dark), reflections::face(dark), velvet::face(dark), freeverb::face(dark), convolution::face(dark)]
}
