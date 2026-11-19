//! What the arrangement should be showing while a take is reviewed.
//!
//! The review screen is not a second timeline: it is the arrangement,
//! scrolled and zoomed to the thing being judged. This works out where
//! that is — the section you are in, with two bars of run-up and one of
//! tail — and hands back an ordinary [`Viewport`], because everything
//! that draws the arrangement already takes one.
//!
//! # Why two before and one after
//!
//! Because a take is judged from its entrance. Whether the chorus
//! landed is a question about the bar before it as much as the chorus,
//! and a window that starts exactly on the downbeat shows you the
//! result without the approach. One bar after is enough to see the
//! ending resolve and not so much that the section stops being the
//! subject.
//!
//! Bars, not seconds, because the run-up is musical: two bars is two
//! bars at any tempo, where four seconds is a bar and a half at 90 and
//! three bars at 180.

use daw_ui::studio::project::TempoChange;

use crate::arrangement::Viewport;
use crate::ruler::Bars;

/// How many bars of run-up the window shows.
pub const BEFORE: f64 = 2.0;

/// How many bars of tail.
pub const AFTER: f64 = 1.0;

/// The shortest window, in seconds.
///
/// A four-bar section at 200 bpm is under five seconds, and a window
/// that short is a wall of waveform with no shape to read. Below this
/// the window grows around the section rather than hugging it.
pub const SHORTEST: f64 = 8.0;

/// The span of session the review should show for a section.
///
/// Clamped at zero: a section at the top of the song cannot show two
/// bars of what came before it, and a window that started at a negative
/// time would draw the count-in twice.
#[must_use]
pub fn span(section: (f64, f64), tempo: &[TempoChange]) -> (f64, f64) {
    let (start, end) = (section.0.min(section.1), section.0.max(section.1));
    // The bar length is read at each END of the window rather than once
    // in the middle: a song that changes tempo at the section boundary —
    // which is where songs change tempo — has a different run-up from
    // its tail, and one number for both would be wrong at whichever end
    // it was not measured at.
    let run_up = Bars::at_time(tempo, start).secs_per_bar() * BEFORE;
    let tail = Bars::at_time(tempo, end).secs_per_bar() * AFTER;
    let (mut from, mut to) = ((start - run_up).max(0.0), end + tail);
    if to - from < SHORTEST {
        // Grown from the middle, so a short section stays in the middle
        // of what you are looking at rather than being pinned to an
        // edge by the arithmetic.
        let middle = (from + to) / 2.0;
        from = (middle - SHORTEST / 2.0).max(0.0);
        to = from + SHORTEST;
    }
    (from, to)
}

/// The viewport that shows that span in a frame this big.
///
/// `width` and `height` are the frame's CONTENT — what is inside the
/// rails — because that is what every other viewport in this window is
/// built from. The span is fitted to the LANE, which is that width
/// minus the track panel: fitting it to the whole content would push
/// the last bar of the section under the panel, where it is drawn over
/// rather than visible.
#[must_use]
pub fn viewport(
    section: (f64, f64),
    tempo: &[TempoChange],
    width: f64,
    height: f64,
    zoom_y: f64,
) -> Viewport {
    let (from, to) = span(section, tempo);
    let seconds = (to - from).max(0.001);
    let lane = (width - crate::arrangement::TCP_WIDTH).max(1.0);
    let pps = (lane / seconds).max(0.001);
    Viewport {
        scroll_x: from * pps,
        scroll_y: 0.0,
        pps,
        zoom_y,
        width,
        height,
        panel_w: crate::arrangement::TCP_WIDTH,
    }
}
