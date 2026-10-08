//! The reverb algorithms' faces, in the block's `algorithm` order.

mod room;
pub(crate) mod hall;
pub(crate) mod plate;
mod spring;
pub(crate) mod cloud;
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
    small_faces(dark, super::kit::Tier::Phone)
}

/// The layouts there are for a desktop pane's lane, by algorithm.
pub fn pane_faces(dark: bool) -> Vec<(usize, Face)> {
    small_faces(dark, super::kit::Tier::Ipad)
}

/// The small-room layouts at `t` (a phone's lane, a desktop pane's), by
/// algorithm: the drawn machines', and the placeholder's for the rest.
pub fn small_faces(dark: bool, t: super::kit::Tier) -> Vec<(usize, Face)> {
    let mut v = vec![(1, hall::face_at(dark, t)), (2, plate::face_at(dark, t)), (4, cloud::face_at(dark, t))];
    v.extend(stub_faces(dark, t));
    v
}

/// The backs there are (the advanced controls), by algorithm, at `t`.
pub fn back_faces(dark: bool, t: super::kit::Tier) -> Vec<(usize, Face)> {
    vec![(1, hall::back_at(dark, t)), (2, plate::back_at(dark, t)), (4, cloud::back_at(dark, t))]
}

pub fn faces(dark: bool) -> Vec<Face> {
    vec![room::face(dark), hall::face(dark), plate::face(dark), spring::face(dark), cloud::face(dark), bloom::face(dark), shimmer::face(dark), chorale::face(dark), magneto::face(dark), nonlinear::face(dark), swell::face(dark), reflections::face(dark), velvet::face(dark), freeverb::face(dark), convolution::face(dark)]
}

/// The machines still drawn as the placeholder, by index: (index, file,
/// name) — in a small room each is [`kit::stub_small`].
const STUBS: [(usize, &str, &str); 12] = [
    (0, "80-verb-room", "Room"),
    (3, "83-verb-spring", "Spring"),
    (5, "85-verb-bloom", "Bloom"),
    (6, "86-verb-shimmer", "Shimmer"),
    (7, "87-verb-chorale", "Chorale"),
    (8, "88-verb-magneto", "Magneto"),
    (9, "89-verb-nonlinear", "NonLinear"),
    (10, "90-verb-swell", "Swell"),
    (11, "91-verb-reflections", "Reflections"),
    (12, "92-verb-velvet", "Velvet"),
    (13, "93-verb-freeverb", "FreeVerb"),
    (14, "94-verb-convolution", "Convolution"),
];

/// The placeholders' small-room faces at `t`, by index.
pub fn stub_faces(dark: bool, t: super::kit::Tier) -> Vec<(usize, Face)> {
    use super::kit;
    STUBS
        .iter()
        .map(|&(i, file, name)| {
            let mut f = kit::reverb_face(kit::tier_file(file, t, dark), kit::stub_small(kit::VERB, name, dark, t));
            f.params.extend(kit::nameplate_params(kit::VERB));
            f.params.extend(kit::time_params(kit::VERB));
            (i, f)
        })
        .collect()
}
