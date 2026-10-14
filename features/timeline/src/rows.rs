//! Rows: how a surface's row space folds onto the slots it draws.

/// A monotonic row -> slot map: hidden rows collapse onto the visible
/// row above them, and everything higher shifts down to close the gap.
///
/// Empty is the identity, which is every mode but drums.
/// A fold holds one hidden row per two-handed piece. The FTS map has
/// five (kick and four toms); the cap is generous and fixed so that
/// `Camera` stays `Copy`, which it is at every call site.
pub const MAX_FOLD: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowFold {
    /// Hidden model rows, ascending, `len` of them meaningful.
    hidden: [i32; MAX_FOLD],
    len: usize,
}

impl Default for RowFold {
    fn default() -> Self {
        Self {
            hidden: [0; MAX_FOLD],
            len: 0,
        }
    }
}

impl RowFold {
    /// `hidden` need not be sorted or unique. Rows past `MAX_FOLD` are
    /// dropped: showing an extra lane beats refusing to draw the roll.
    pub fn new(mut hidden: Vec<i32>) -> Self {
        hidden.sort_unstable();
        hidden.dedup();
        hidden.truncate(MAX_FOLD);
        let mut rows = [0; MAX_FOLD];
        rows[..hidden.len()].copy_from_slice(&hidden);
        Self {
            hidden: rows,
            len: hidden.len(),
        }
    }

    fn rows(&self) -> &[i32] {
        &self.hidden[..self.len]
    }

    pub fn is_identity(&self) -> bool {
        self.len == 0
    }

    /// How many rows this fold hides — the difference between the row
    /// range and the number of lanes actually drawn.
    pub fn hidden_count(&self) -> usize {
        self.len
    }

    fn is_hidden(&self, row: i32) -> bool {
        self.rows().binary_search(&row).is_ok()
    }

    /// Hidden rows strictly below `row`.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "at most MAX_FOLD"
    )]
    fn below(&self, row: i32) -> i32 {
        self.rows().partition_point(|&h| h < row) as i32
    }

    /// Display slot for a model row. Fractional parts survive, so a
    /// note bent a quarter-row sharp still draws a quarter-row up.
    #[expect(clippy::cast_possible_truncation, reason = "a row index")]
    #[must_use]
    pub fn slot(&self, row: f64) -> f64 {
        if self.len == 0 {
            return row;
        }
        let base = row.floor();
        let frac = row - base;
        let mut r = base as i32;
        // A hidden row shares the slot of the first visible row above
        // it — the fold only ever hides a left hand, which sits
        // directly below its right.
        while self.is_hidden(r) {
            r += 1;
        }
        (r - self.below(r)) as f64 + frac
    }

    /// The model row occupying a display slot — the inverse of `slot`
    /// for visible rows, and the reason a click on a collapsed piece
    /// lands on the hand that stands for it.
    #[expect(clippy::cast_possible_truncation, reason = "a slot index")]
    #[must_use]
    pub fn row(&self, slot: f64) -> f64 {
        if self.len == 0 {
            return slot;
        }
        let base = slot.floor();
        let frac = slot - base;
        let mut r = base as i32;
        for &h in self.rows() {
            if h <= r {
                r += 1;
            } else {
                break;
            }
        }
        r as f64 + frac
    }

    /// A row-space range expressed in slots.
    #[expect(clippy::cast_possible_truncation, reason = "a slot index")]
    #[must_use]
    pub fn slot_bounds(&self, (lo, hi): (i32, i32)) -> (i32, i32) {
        (self.slot(lo as f64) as i32, self.slot(hi as f64) as i32)
    }
}
