//! The drag lifecycle and the arithmetic of moving and trimming spans,
//! shared by the arrangement's items and the Editor's notes.
//!
//! Every drag has the same shape: nothing happens at the press, it
//! **begins** when the pointer has travelled past [`SLOP`] (so a click
//! edits nothing and leaves no undo step), it is **live** with the total
//! change from the press, and it **commits** or is **cancelled**.

use crate::gestures::SLOP;

/// Where a drag is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The pointer just crossed the slop: the first change.
    Begin,
    /// A later change, still in flight.
    Live,
    /// Let go after it began.
    Commit,
    /// Let go before it began (a click), or abandoned.
    Cancel,
}

/// Decides when a press becomes a drag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragGate {
    from: (f64, f64),
    begun: bool,
}

impl DragGate {
    /// A press at `at` (screen px).
    #[must_use]
    pub const fn press(at: (f64, f64)) -> Self {
        Self {
            from: at,
            begun: false,
        }
    }

    /// The pointer moved to `at`: `None` while still within the slop,
    /// then [`Phase::Begin`] once, then [`Phase::Live`].
    pub fn moved(&mut self, at: (f64, f64)) -> Option<Phase> {
        if self.begun {
            return Some(Phase::Live);
        }
        let far = (at.0 - self.from.0).abs() > SLOP || (at.1 - self.from.1).abs() > SLOP;
        far.then(|| {
            self.begun = true;
            Phase::Begin
        })
    }

    /// Whether the drag has begun.
    #[must_use]
    pub const fn begun(&self) -> bool {
        self.begun
    }

    /// Let go: [`Phase::Commit`] if it began, else [`Phase::Cancel`].
    #[must_use]
    pub const fn release(self) -> Phase {
        if self.begun {
            Phase::Commit
        } else {
            Phase::Cancel
        }
    }
}

/// A span `(start, end)` with one edge moved to `to` (already snapped, or
/// not): the start never past `end - min_len` nor before `floor`, the end
/// never before `start + min_len`.
#[must_use]
pub fn trim(span: (f64, f64), start_edge: bool, to: f64, min_len: f64, floor: f64) -> (f64, f64) {
    let (start, end) = span;
    if start_edge {
        (to.max(floor).min(end - min_len), end)
    } else {
        (start, to.max(start + min_len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_begins_past_the_slop_and_a_click_cancels() {
        let mut g = DragGate::press((10.0, 10.0));
        assert_eq!(g.moved((12.0, 11.0)), None);
        assert_eq!(g.moved((20.0, 10.0)), Some(Phase::Begin));
        assert_eq!(
            g.moved((19.0, 10.0)),
            Some(Phase::Live),
            "once begun, always live"
        );
        assert_eq!(g.release(), Phase::Commit);
        let click = DragGate::press((0.0, 0.0));
        assert_eq!(click.release(), Phase::Cancel);
    }

    #[test]
    fn a_trim_keeps_its_length_and_its_floor() {
        assert_eq!(trim((2.0, 6.0), true, 3.0, 0.01, 0.0), (3.0, 6.0));
        assert_eq!(trim((2.0, 6.0), true, 9.0, 0.01, 0.0), (5.99, 6.0));
        assert_eq!(trim((2.0, 6.0), true, -1.0, 0.01, 0.0), (0.0, 6.0));
        assert_eq!(trim((2.0, 6.0), false, 1.0, 0.5, 0.0), (2.0, 2.5));
        assert_eq!(trim((2.0, 6.0), false, 8.0, 0.5, 0.0), (2.0, 8.0));
    }
}
