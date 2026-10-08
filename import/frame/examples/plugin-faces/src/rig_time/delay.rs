//! The delay machines' faces, in the block's `style` order.

pub(crate) mod tape;
pub(crate) mod digital;
pub(crate) mod analog;
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
    let mut v = vec![(0, tape::face_at(dark, t)), (1, digital::face_at(dark, t)), (2, analog::face_at(dark, t))];
    v.extend(stub_faces(dark, t));
    v
}

/// The backs there are (the advanced controls), by algorithm, at `t`.
pub fn back_faces(dark: bool, t: super::kit::Tier) -> Vec<(usize, Face)> {
    vec![(0, tape::back_at(dark, t)), (1, digital::back_at(dark, t)), (2, analog::back_at(dark, t))]
}

pub fn faces(dark: bool) -> Vec<Face> {
    vec![tape::face(dark), digital::face(dark), analog::face(dark), lo_fi::face(dark), shimmer::face(dark), reverse::face(dark), ice::face(dark), rhythm::face(dark), drum::face(dark), oil_can::face(dark), multitap::face(dark), spectral::face(dark), filter::face(dark)]
}

/// The machines still drawn as the placeholder, by index: (index, file,
/// name) — in a small room each is [`kit::stub_small`].
const STUBS: [(usize, &str, &str); 10] = [
    (3, "63-dly-lo-fi", "Lo-Fi"),
    (4, "64-dly-shimmer", "Shimmer"),
    (5, "65-dly-reverse", "Reverse"),
    (6, "66-dly-ice", "Ice"),
    (7, "67-dly-rhythm", "Rhythm"),
    (8, "68-dly-drum", "Drum"),
    (9, "69-dly-oil-can", "Oil Can"),
    (10, "70-dly-multitap", "MultiTap"),
    (11, "71-dly-spectral", "Spectral"),
    (12, "72-dly-filter", "Filter"),
];

/// The placeholders' small-room faces at `t`, by index.
pub fn stub_faces(dark: bool, t: super::kit::Tier) -> Vec<(usize, Face)> {
    use super::kit;
    STUBS
        .iter()
        .map(|&(i, file, name)| {
            let mut f = kit::delay_face(kit::tier_file(file, t, dark), kit::stub_small(kit::DLY, name, dark, t));
            f.params.extend(kit::nameplate_params(kit::DLY));
            f.params.extend(kit::time_params(kit::DLY));
            (i, f)
        })
        .collect()
}
