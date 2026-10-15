//! Drum leveling: the kit's profile on [`crate::events`].
//!
//! A drum is struck, so it is leveled by events — one gain a stroke, read
//! at its attack, held through its ring. What is drum-specific is only
//! which events ride and how hard: a ghost note, bleed from another drum,
//! a flam's grace is no stroke and rides under the stroke before it; a
//! kick is evened most, a tom least, so fills keep their shape.

use crate::events::Ride;

/// A drum's hit, as the leveler reads it: an event that rides when it is
/// a stroke.
pub type Hit = crate::events::Event;

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
    crate::events::target_at(hits, t, AROUND)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{gain, ride};

    /// A ghost is not leveled up to a stroke: it rides under the gain of
    /// the stroke before it.
    #[test]
    fn a_ghost_stays_a_ghost() {
        let stroke = |t_s| Hit {
            t_s,
            level_db: -16.0,
            rides: true,
        };
        let ghost = Hit {
            t_s: 1.2,
            level_db: -40.0,
            rides: false,
        };
        assert!(ride(&[stroke(1.0), ghost, stroke(2.0)], |_| -16.0, &SNARE).is_empty());
    }

    /// Each drum has its own ride; a hand's knobs turn it.
    #[test]
    fn each_drum_has_its_own_ride() {
        assert_eq!(ride_of("Kick In"), KICK);
        assert_eq!(ride_of("Snare"), SNARE);
        assert_eq!(ride_of("Tom 3"), TOM);
        assert!((gain(-40.0, -10.0, &TOM) - TOM.max_gain_db).abs() < 1e-9);
        assert!((gain(0.0, -30.0, &TOM) + TOM.max_cut_db).abs() < 1e-9);
        let all = KICK.turned(Some(1.0), Some(24.0));
        assert!((gain(-30.0, -20.0, &all) - 10.0).abs() < 1e-12);
    }
}
