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
mod shell;

use crate::face::Face;

/// Every delay face, in `kit::DELAY_ALGOS` order.
#[must_use]
/// The phone layouts there are, by machine (its index in `DELAY_ALGOS`'s
/// order, as [`faces`] lists them).
pub fn phone_faces(dark: bool) -> Vec<(usize, Face)> {
    vec![(0, tape::face_at(dark, super::kit::Tier::Phone)), (2, analog::face_at(dark, super::kit::Tier::Phone)), (3, lo_fi::face_at(dark, super::kit::Tier::Phone)), (4, shimmer::face_at(dark, super::kit::Tier::Phone)), (5, reverse::face_at(dark, super::kit::Tier::Phone)), (6, ice::face_at(dark, super::kit::Tier::Phone)), (7, rhythm::face_at(dark, super::kit::Tier::Phone))]
}

/// The backs there are (the advanced controls), by algorithm, at `t`.
pub fn back_faces(dark: bool, t: super::kit::Tier) -> Vec<(usize, Face)> {
    vec![(0, tape::back_at(dark, t)), (2, analog::back_at(dark, t)), (3, lo_fi::back_at(dark, t)), (4, shimmer::back_at(dark, t)), (5, reverse::back_at(dark, t)), (6, ice::back_at(dark, t)), (7, rhythm::back_at(dark, t))]
}

/// Every delay's tall face (see `kit::tall_chrome`), in `DELAY_ALGOS`
/// order: its own where it has one, else the placeholder's.
#[must_use]
pub fn tall_faces(dark: bool) -> Vec<Face> {
    faces(false)
        .into_iter()
        .zip(super::kit::DELAY_ALGOS)
        .map(|(f, name)| match name {
            "Tape" => tape::tall(dark),
            "Analog" => analog::tall(dark),
            "Lo-Fi" => lo_fi::tall(dark),
            "Shimmer" => shimmer::tall(dark),
            "Reverse" => reverse::tall(dark),
            "Ice" => ice::tall(dark),
            "Rhythm" => rhythm::tall(dark),
            _ => super::kit::stub_tall(super::kit::DLY, name, f.file, dark),
        })
        .collect()
}

pub fn faces(dark: bool) -> Vec<Face> {
    vec![tape::face(dark), digital::face(dark), analog::face(dark), lo_fi::face(dark), shimmer::face(dark), reverse::face(dark), ice::face(dark), rhythm::face(dark), drum::face(dark), oil_can::face(dark), multitap::face(dark), spectral::face(dark), filter::face(dark)]
}
