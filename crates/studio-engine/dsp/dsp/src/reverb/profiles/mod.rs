//! Reverb profiles — the seven families a reverb comes in, and which engine
//! each one is.
//!
//! `reverb-dsp` ships fifteen algorithms and their variants, which is a list of
//! implementations, not a list of *spaces*. Nobody reaching for a reverb wants
//! "Velvet" or "`FreeVerb`"; they want a hall, or a plate, or something strange.
//! So the panel is organised the way the question is asked:
//!
//! IR · Hall · Plate · Room · Spring · Ambient · Special
//!
//! Every algorithm falls into one of those. The first five are the physical
//! spaces and each holds the variants of its engine (a hall is Concert,
//! Cathedral or Arena). Ambient holds the washes that are not a room at all —
//! clouds, blooms, swells. Special holds the ones that are effects wearing a
//! reverb's clothes: shimmer, chorale, magneto, non-linear.
//!
//! This crate is pure data: which profiles exist, which family each belongs to,
//! and which engine and variant it selects. No GUI, no framework deps — the
//! same split `comp-profiles` has, and for the same reason.

use crate::reverb::AlgorithmType;

/// One selectable reverb: a name, the engine behind it, and the family it
/// lives in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Profile {
    /// Stable id. **Persisted** — a session records this, never an index, so
    /// adding or reordering profiles cannot change what an old project opens
    /// with. Same contract as the compressor's `profile_id`.
    pub id: &'static str,
    /// What the rail and the panel call it.
    pub name: &'static str,
    /// The engine it selects.
    pub algorithm: AlgorithmType,
    /// Which variant of that engine, where it has more than one.
    pub variant: usize,
    /// One line on what it is for. Panels are small; this is the tooltip.
    pub voice: &'static str,
}

/// A rail entry: one family, and the profiles that cycle inside it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Category {
    /// Stable id, used for test ids and rail keys.
    pub id: &'static str,
    /// "Spring".
    pub label: &'static str,
    /// Rail badge when nothing in the family is active.
    pub badge: &'static str,
    /// Profile ids, in cycling order — clicking the active family advances
    /// through them.
    pub profiles: &'static [&'static str],
}

pub static PROFILES: &[Profile] = &[
    // ── IR ───────────────────────────────────────────────────────────────
    Profile {
        id: "ir",
        name: "Impulse Response",
        algorithm: AlgorithmType::Convolution,
        variant: 0,
        voice: "A recorded space, played back. Whatever was captured, exactly.",
    },
    // ── Hall ─────────────────────────────────────────────────────────────
    Profile {
        id: "hall_concert",
        name: "Concert Hall",
        algorithm: AlgorithmType::Hall,
        variant: 0,
        voice: "A room built for an orchestra: long, even, and behind the source.",
    },
    Profile {
        id: "hall_cathedral",
        name: "Cathedral",
        algorithm: AlgorithmType::Hall,
        variant: 1,
        voice: "Stone. Very long, very diffuse, and slow to arrive.",
    },
    Profile {
        id: "hall_arena",
        name: "Arena",
        algorithm: AlgorithmType::Hall,
        variant: 2,
        voice: "Big and hard-walled — the tail of a room too large for its surfaces.",
    },
    // ── Plate ────────────────────────────────────────────────────────────
    Profile {
        id: "plate_classic",
        name: "Plate",
        algorithm: AlgorithmType::Plate,
        variant: 0,
        voice: "Sheet steel: dense from the first millisecond, no early reflections.",
    },
    Profile {
        id: "plate_224",
        name: "Digital Plate",
        algorithm: AlgorithmType::Plate,
        variant: 1,
        voice: "The early-digital plate — grainy, wide, and unmistakably a machine.",
    },
    Profile {
        id: "plate_progenitor",
        name: "Modern Plate",
        algorithm: AlgorithmType::Plate,
        variant: 2,
        voice: "A plate with the grain taken out: smooth, tall, and very clean.",
    },
    // ── Room ─────────────────────────────────────────────────────────────
    Profile {
        id: "room_medium",
        name: "Room",
        algorithm: AlgorithmType::Room,
        variant: 0,
        voice: "Somewhere real and unremarkable — the sound of not being in a booth.",
    },
    Profile {
        id: "room_chamber",
        name: "Chamber",
        algorithm: AlgorithmType::Room,
        variant: 1,
        voice: "A hard-walled room built to be reverberant. Between a room and a hall.",
    },
    Profile {
        id: "room_studio",
        name: "Studio",
        algorithm: AlgorithmType::Room,
        variant: 2,
        voice: "Small, treated, and short. Depth without a tail.",
    },
    // ── Spring ───────────────────────────────────────────────────────────
    Profile {
        id: "spring_classic",
        name: "Spring",
        algorithm: AlgorithmType::Spring,
        variant: 0,
        voice: "Coils: boingy, mid-forward, and happiest on a guitar.",
    },
    Profile {
        id: "spring_vintage",
        name: "Vintage Spring",
        algorithm: AlgorithmType::Spring,
        variant: 1,
        voice: "The tank in an old amp — narrower, dirtier, and prone to drip.",
    },
    // ── Ambient ──────────────────────────────────────────────────────────
    Profile {
        id: "cloud",
        name: "Cloud",
        algorithm: AlgorithmType::Cloud,
        variant: 0,
        voice: "No walls. A wash that keeps going and never resolves to a room.",
    },
    Profile {
        id: "bloom",
        name: "Bloom",
        algorithm: AlgorithmType::Bloom,
        variant: 0,
        voice: "Swells in after the note, then leaves. A reverb with an envelope.",
    },
    Profile {
        id: "swell",
        name: "Swell",
        algorithm: AlgorithmType::Swell,
        variant: 0,
        voice: "Attack taken off the front entirely — pads out of anything.",
    },
    Profile {
        id: "velvet",
        name: "Velvet",
        algorithm: AlgorithmType::Velvet,
        variant: 0,
        voice: "Sparse noise instead of a network: soft, smeared, and very quiet.",
    },
    // ── Special ──────────────────────────────────────────────────────────
    Profile {
        id: "shimmer",
        name: "Shimmer",
        algorithm: AlgorithmType::Shimmer,
        variant: 0,
        voice: "The tail is pitched up as it decays. Endless, and not of this world.",
    },
    Profile {
        id: "chorale",
        name: "Chorale",
        algorithm: AlgorithmType::Chorale,
        variant: 0,
        voice: "Voices in the tail — harmonised reflections rather than plain ones.",
    },
    Profile {
        id: "magneto",
        name: "Magneto",
        algorithm: AlgorithmType::Magneto,
        variant: 0,
        voice: "Tape delay feeding a room: wow, flutter and saturation in the tail.",
    },
    Profile {
        id: "nonlinear",
        name: "Non-Linear",
        algorithm: AlgorithmType::NonLinear,
        variant: 0,
        voice: "Gated: the tail is cut flat rather than allowed to decay.",
    },
    Profile {
        id: "reflections",
        name: "Reflections",
        algorithm: AlgorithmType::Reflections,
        variant: 0,
        voice: "Early reflections only. Position and size, with no tail at all.",
    },
    Profile {
        id: "freeverb",
        name: "Freeverb",
        algorithm: AlgorithmType::FreeVerb,
        variant: 0,
        voice: "The Schroeder box everyone started with. Plain, cheap, and honest.",
    },
    Profile {
        id: "random",
        name: "Random",
        algorithm: AlgorithmType::Random,
        variant: 0,
        voice: "Delay lines that wander. Never repeats, so it never rings or beats.",
    },
];

/// The rail, in order.
///
/// IR first because it is a different kind of answer to the question — a
/// recording rather than a model — and the rest run from the most room-like to
/// the least.
pub static CATEGORIES: &[Category] = &[
    Category {
        id: "ir",
        label: "IR",
        badge: "IR",
        profiles: &["ir"],
    },
    Category {
        id: "hall",
        label: "Hall",
        badge: "HALL",
        profiles: &["hall_concert", "hall_cathedral", "hall_arena"],
    },
    Category {
        id: "random",
        label: "Random",
        badge: "RAND",
        profiles: &["random"],
    },
    Category {
        id: "plate",
        label: "Plate",
        badge: "PLT",
        profiles: &["plate_classic", "plate_224", "plate_progenitor"],
    },
    Category {
        id: "room",
        label: "Room",
        badge: "ROOM",
        profiles: &["room_medium", "room_chamber", "room_studio"],
    },
    Category {
        id: "spring",
        label: "Spring",
        badge: "SPR",
        profiles: &["spring_classic", "spring_vintage"],
    },
    Category {
        id: "ambient",
        label: "Ambient",
        badge: "AMB",
        profiles: &["cloud", "bloom", "swell", "velvet"],
    },
    Category {
        id: "special",
        label: "Special",
        badge: "SPC",
        profiles: &[
            "shimmer",
            "chorale",
            "magneto",
            "nonlinear",
            "reflections",
            "freeverb",
        ],
    },
];

/// The profile an id names, if this build has it.
#[must_use]
pub fn profile_by_id(id: &str) -> Option<&'static Profile> {
    PROFILES.iter().find(|p| p.id == id)
}

/// Where an id sits in [`PROFILES`].
#[must_use]
pub fn profile_index(id: &str) -> Option<usize> {
    PROFILES.iter().position(|p| p.id == id)
}

/// The family a profile belongs to, and its position inside it.
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

/// The profile index a rail click selects.
///
/// Clicking the family you are already in advances to the next profile inside
/// it and wraps; clicking any other family lands on its first. Same idiom as
/// the compressor's rail, so the two plugins are the same instrument.
#[must_use]
pub fn rail_click_target(current_index: usize, clicked_category: usize) -> usize {
    let current_id = PROFILES.get(current_index).map_or("", |p| p.id);
    let Some(category) = CATEGORIES.get(clicked_category) else {
        return current_index;
    };
    let next_id = match category_of(current_id) {
        Some((ci, vi)) if ci == clicked_category => {
            category.profiles[(vi + 1) % category.profiles.len()]
        }
        _ => category.profiles[0],
    };
    profile_index(next_id).unwrap_or(current_index)
}
