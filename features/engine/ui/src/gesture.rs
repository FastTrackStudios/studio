//! Turning pointer events into things that happened.
//!
//! A window gets presses, moves and releases. A mixer needs clicks,
//! double-clicks and drags — and the difference between them is timing
//! and distance, not anything the window can tell you. That part is
//! shared with the arrangement and the Editor: [`timeline::input::Clicks`]
//! (a click is decided on release; see there for why).
//!
//! # Fine adjustment
//!
//! A fader is a few hundred pixels and its range is sixty decibels, so
//! one pixel is a fifth of a dB — fine enough to mix with and far too
//! coarse to match a level by ear. Holding the fine modifier scales the
//! movement down rather than changing what is being dragged, which is
//! REAPER's behaviour and everyone else's.

use crate::hit::Hit;

pub use timeline::input::{DOUBLE, SLOP};

/// How much a drag is scaled by while the fine modifier is held.
///
/// A quarter. Enough that a whole fader's travel becomes four screens
/// of movement — which sounds absurd until you are matching a level by
/// ear, where the useful range is about two dB.
pub const FINE: f64 = 0.25;

/// What the pointer did, over a mixer [`Hit`].
pub type Event = timeline::input::ClickEvent<Hit>;

/// The pointer's state between events, over mixer [`Hit`]s. Pass
/// [`FINE`] as `moved`'s scale while the fine modifier is held.
pub type Gestures = timeline::input::Clicks<Hit>;

/// How far a vertical drag moves a control, as a fraction of its range.
///
/// `travel` is what the control has on screen: a fader's groove, or the
/// notional travel a knob is worth. Dividing by it means a control
/// twice as tall takes twice the movement, which is what makes a big
/// fader feel precise and a small one feel quick.
///
/// UP is positive, because down is negative in screen coordinates and
/// every control in a mixer goes up to mean more.
#[must_use]
pub fn drag_fraction(delta_y: f64, travel: f64) -> f64 {
    if travel <= 0.0 {
        return 0.0;
    }
    -delta_y / travel
}
