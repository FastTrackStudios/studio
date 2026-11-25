//! The theme tokens the arrangement draws with (`THEME_TOKENS`): the kit's
//! palette tokens it reads, and its own named colours, each with the
//! colour it has when no theme names it
//! ([`daw_view_kit::theme_tokens`]).

use daw_edit::drums::LaneRole;
use daw_view_kit::theme_tokens::ThemeToken;

/// A shut kit's drum envelope, kick.
pub const DRUM_KICK: ThemeToken = ThemeToken {
    name: "arrangement.drum.kick",
    default: "#ef4444",
    what: "a folded kit item's kick envelope",
};
/// A shut kit's drum envelope, snare.
pub const DRUM_SNARE: ThemeToken = ThemeToken {
    name: "arrangement.drum.snare",
    default: "#f59e0b",
    what: "a folded kit item's snare envelope",
};
/// A shut kit's drum envelope, toms.
pub const DRUM_TOMS: ThemeToken = ThemeToken {
    name: "arrangement.drum.toms",
    default: "#a3e635",
    what: "a folded kit item's toms envelope",
};
/// A shut kit's drum envelope, the rest (hats, cymbals, rooms).
pub const DRUM_OTHER: ThemeToken = ThemeToken {
    name: "arrangement.drum.other",
    default: "#22d3ee",
    what: "a folded kit item's envelope of the rest (hats, cymbals, rooms)",
};

/// The token a drum role's envelope is drawn in.
#[must_use]
pub const fn drum(role: LaneRole) -> &'static ThemeToken {
    match role {
        LaneRole::Kick => &DRUM_KICK,
        LaneRole::Snare => &DRUM_SNARE,
        LaneRole::Toms => &DRUM_TOMS,
        LaneRole::Other => &DRUM_OTHER,
    }
}

/// Every token the arrangement draws with: the kit's palette tokens it
/// reads by name, then its own.
pub const THEME_TOKENS: &[ThemeToken] = &[DRUM_KICK, DRUM_SNARE, DRUM_TOMS, DRUM_OTHER];
