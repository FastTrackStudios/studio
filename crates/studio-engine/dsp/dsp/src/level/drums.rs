//! Drum leveling: the kit's profile on [`crate::level::events`].
//!
//! A drum is struck, so it is leveled by events — one gain a stroke, read
//! at its attack, held through its ring. What is drum-specific is only
//! which events ride and how hard: a ghost note, bleed from another drum,
//! a flam's grace is no stroke and rides under the stroke before it; a
//! kick is evened most, a tom least, so fills keep their shape.

use crate::level::events::Ride;

/// A drum's hit, as the leveler reads it: an event that rides when it is
/// a stroke.
pub type Hit = crate::level::events::Event;

/// How many strokes around one its level is held up to: about two bars
/// of a busy kick. A quiet verse stays quieter than the chorus — only
/// stroke to stroke is the drum made even.
pub const AROUND: usize = 16;

/// A kick: the most even of the kit — a steady floor under the song, its
/// strokes brought most of the way together.
pub const KICK: Ride = Ride {
    amount: 0.75,
    max_gain_db: 6.0,
    max_cut_db: 6.0,
    lead_s: 0.004,
    ramp_s: 0.001,
    least_db: 0.1,
};

/// A snare: even backbeats, with the accents still above them.
pub const SNARE: Ride = Ride {
    amount: 0.6,
    ..KICK
};

/// A tom: fills keep their shape — half the spread taken out.
pub const TOM: Ride = Ride {
    amount: 0.5,
    max_gain_db: 4.0,
    ..KICK
};

/// The ride for a drum by its piece's name.
#[must_use]
pub fn ride_of(piece: &str) -> Ride {
    let name = piece.to_lowercase();
    if name.contains("kick") {
        KICK
    } else if name.contains("snare") {
        SNARE
    } else {
        TOM
    }
}

/// The level the strokes around `t` sit at (see [`AROUND`]).
#[must_use]
pub fn target_at(hits: &[Hit], t: f64) -> Option<f64> {
    crate::level::events::target_at(hits, t, AROUND)
}
