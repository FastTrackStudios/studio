//! The rails' geometry, which every view in a window lays out by: how
//! wide a side rail is and how tall the top one is. The rails themselves
//! (what they hold) are the mixer's.

/// How wide the side rails are.
///
/// Enough for a 22-pixel icon with a margin either side, which is the
/// size REAPER's own toolbar buttons are and the size the ported art is
/// authored at. A rail narrower than its buttons is a rail you miss.
pub const SIDE: f64 = 44.0;

/// And how tall the top one is.
pub const TOP: f64 = 30.0;
