//! Compressor hardware profiles — parameter mappings and constraints.
//!
//! A profile defines:
//! - Which controls appear (knobs, switches, stepped selectors)
//! - How each control maps to [`crate::comp::CompChain`] parameters
//! - Constraints (locked ratios, attack/release curves, linked params)
//!
//! Profiles are pure data + mapping functions. No GUI, no framework deps.

pub mod cl1b;
pub mod control;
pub mod core;
pub mod dbx160;
pub mod distressor;
pub mod fairchild;
pub mod la2a;
pub mod manley;
pub mod presets;
pub mod ssl_bus;
pub mod urei_1176;

pub use self::core::{Constraint, ParamMapping, Profile, ProfileControl};
pub use cl1b::Cl1bProfile;
pub use control::ControlProfile;
pub use dbx160::Dbx160Profile;
pub use distressor::DistressorProfile;
pub use fairchild::Fairchild670Profile;
pub use la2a::La2aProfile;
pub use manley::ManleyVariMuProfile;
pub use presets::{FACTORY_PRESETS, FactoryPreset, PresetParam, all_factory_presets};
pub use ssl_bus::SslBusProfile;
pub use urei_1176::Urei1176Profile;

pub static CONTROL: ControlProfile = ControlProfile;
pub static LA2A: La2aProfile = La2aProfile;
pub static CL1B: Cl1bProfile = Cl1bProfile;
pub static FAIRCHILD_670: Fairchild670Profile = Fairchild670Profile;
pub static MANLEY_VARI_MU: ManleyVariMuProfile = ManleyVariMuProfile;
pub static SSL_BUS: SslBusProfile = SslBusProfile;
pub static DBX_160: Dbx160Profile = Dbx160Profile;
pub static DISTRESSOR: DistressorProfile = DistressorProfile;
/// The three finishes the FET limiter came in. Same circuit, same controls —
/// see [`Urei1176Profile`].
pub static UREI_1176: Urei1176Profile = Urei1176Profile {
    id: "urei_1176",
    name: "FET Limiter · Blackface",
};
pub static UREI_1176_SILVER: Urei1176Profile = Urei1176Profile {
    id: "urei_1176_silver",
    name: "FET Limiter · Silver",
};
pub static UREI_1176_LN: Urei1176Profile = Urei1176Profile {
    id: "urei_1176_ln",
    name: "FET Limiter · LN",
};

/// Every profile, in the order the UI lists them: the FTS surface first, then
/// each compressor family, and within a family the units in the order the
/// category cycles through them.
///
/// This order IS the `profile` parameter's value order, so **append only** —
/// hosts persist the parameter, and reordering silently repoints a saved
/// session at a different unit.
#[must_use]
pub fn all_profiles() -> [&'static (dyn Profile + Sync); 11] {
    [
        &CONTROL,
        &UREI_1176,
        &LA2A,
        &CL1B,
        &FAIRCHILD_670,
        &MANLEY_VARI_MU,
        &SSL_BUS,
        &DBX_160,
        &DISTRESSOR,
        // Appended: the parameter's value order is this order, and the id is
        // what a session restores from either way.
        &UREI_1176_SILVER,
        &UREI_1176_LN,
        // NOTE: keep in sync with `CATEGORIES` below — the test
        // `every_profile_belongs_to_exactly_one_category` is the guard.
    ]
}

/// A compressor family — how the units are grouped in the UI, and the shape of
/// the rail: one button per family, clicking it again cycles the units inside.
///
/// The grouping is by *topology*, because that is what actually predicts how a
/// unit behaves: a FET is fast and gritty, an opto is slow and self-correcting,
/// a variable-mu glues, a VCA punches. Which specific unit you want inside the
/// family is a finer decision than which family you want, so it is the second
/// click rather than the first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Category {
    /// Stable id, used for test ids and rail keys.
    pub id: &'static str,
    /// "Opto".
    pub label: &'static str,
    /// Rail badge when no unit in the family is active.
    pub badge: &'static str,
    /// Profile ids, in cycling order.
    pub profiles: &'static [&'static str],
}

pub static CATEGORIES: &[Category] = &[
    Category {
        id: "main",
        label: "Main",
        badge: "MAIN",
        profiles: &["control"],
    },
    Category {
        id: "fet",
        label: "FET",
        badge: "FET",
        // Three finishes of one unit — the second click changes the paint,
        // not the circuit.
        profiles: &["urei_1176_silver", "urei_1176", "urei_1176_ln"],
    },
    Category {
        id: "opto",
        label: "Opto",
        badge: "OPT",
        profiles: &["la2a", "cl1b"],
    },
    Category {
        id: "vari_mu",
        label: "Vari-Mu",
        badge: "MU",
        profiles: &["fairchild670", "manley_vari_mu"],
    },
    Category {
        id: "vca",
        label: "VCA",
        badge: "VCA",
        profiles: &["ssl_bus", "dbx160"],
    },
    Category {
        id: "hybrid",
        label: "Hybrid",
        badge: "HYB",
        profiles: &["distressor"],
    },
];

/// The profile with this id, if there is one.
#[must_use]
pub fn profile_by_id(id: &str) -> Option<&'static (dyn Profile + Sync)> {
    all_profiles().into_iter().find(|p| p.id() == id)
}

/// Index of a profile id in [`all_profiles`] — the value the `profile`
/// parameter holds.
#[must_use]
pub fn profile_index(id: &str) -> Option<usize> {
    all_profiles().iter().position(|p| p.id() == id)
}

/// The category a profile belongs to, and its position within it.
#[must_use]
pub fn category_of(profile_id: &str) -> Option<(usize, usize)> {
    CATEGORIES.iter().enumerate().find_map(|(ci, category)| {
        category
            .profiles
            .iter()
            .position(|id| *id == profile_id)
            .map(|vi| (ci, vi))
    })
}

pub fn map_control_value(
    profile: &dyn Profile,
    control_id: &str,
    normalized_value: f64,
) -> Option<Vec<(&'static str, f64)>> {
    let control = profile
        .controls()
        .iter()
        .find(|control| control.id == control_id)?;
    let normalized_value = normalized_value.clamp(0.0, 1.0);

    match &control.mapping {
        ParamMapping::Direct { param, range } => {
            let start = *range.start();
            let end = *range.end();
            Some(vec![(
                *param,
                (end - start).mul_add(normalized_value, start),
            )])
        }
        ParamMapping::Stepped { param, values, .. } => {
            if values.is_empty() {
                return Some(Vec::new());
            }
            let index = (normalized_value * (values.len() - 1) as f64).round() as usize;
            Some(vec![(*param, values[index])])
        }
        ParamMapping::Compound { mappings, .. } => Some(
            mappings
                .iter()
                .map(|(param, map)| (*param, map(normalized_value)))
                .collect(),
        ),
    }
}
