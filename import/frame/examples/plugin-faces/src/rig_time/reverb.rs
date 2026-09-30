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
pub fn faces() -> Vec<Face> {
    vec![room::face(), hall::face(), plate::face(), spring::face(), cloud::face(), bloom::face(), shimmer::face(), chorale::face(), magneto::face(), nonlinear::face(), swell::face(), reflections::face(), velvet::face(), freeverb::face(), convolution::face()]
}
