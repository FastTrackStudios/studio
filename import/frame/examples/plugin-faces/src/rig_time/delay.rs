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
mod machine;

use crate::face::Face;

/// Every delay face, in `kit::DELAY_ALGOS` order.
#[must_use]
/// The phone layouts there are, by machine (its index in `DELAY_ALGOS`'s
/// order, as [`faces`] lists them).
pub fn phone_faces(dark: bool) -> Vec<(usize, Face)> {
    vec![(0, tape::face_at(dark, super::kit::Tier::Phone)), (2, analog::face_at(dark, super::kit::Tier::Phone)), (8, drum::face_at(dark, super::kit::Tier::Phone)), (9, oil_can::face_at(dark, super::kit::Tier::Phone)), (10, multitap::face_at(dark, super::kit::Tier::Phone)), (11, spectral::face_at(dark, super::kit::Tier::Phone)), (12, filter::face_at(dark, super::kit::Tier::Phone))]
}

/// The backs there are (the advanced controls), by algorithm, at `t`.
pub fn back_faces(dark: bool, t: super::kit::Tier) -> Vec<(usize, Face)> {
    vec![(0, tape::back_at(dark, t)), (2, analog::back_at(dark, t)), (8, drum::back_at(dark, t)), (9, oil_can::back_at(dark, t)), (10, multitap::back_at(dark, t)), (11, spectral::back_at(dark, t)), (12, filter::back_at(dark, t))]
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
            "Drum" => drum::tall(dark),
            "Oil Can" => oil_can::tall(dark),
            "MultiTap" => multitap::tall(dark),
            "Spectral" => spectral::tall(dark),
            "Filter" => filter::tall(dark),
            _ => super::kit::stub_tall(super::kit::DLY, name, f.file, dark),
        })
        .collect()
}

pub fn faces(dark: bool) -> Vec<Face> {
    vec![tape::face(dark), digital::face(dark), analog::face(dark), lo_fi::face(dark), shimmer::face(dark), reverse::face(dark), ice::face(dark), rhythm::face(dark), drum::face(dark), oil_can::face(dark), multitap::face(dark), spectral::face(dark), filter::face(dark)]
}
