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

/// What a row is: the arrangement's planned tracks (heights vary, drawn
/// top-down), the Editor's row space (one height, drawn bottom-up).
pub trait RowModel {
    /// Rows in the model's own index space, `lo..=hi`.
    fn bounds(&self) -> (i32, i32);
    /// A row's height in row units, before vertical zoom.
    fn height(&self, row: i32) -> f64;
    /// Whether an item may be dragged from one row to another.
    fn accepts_move(&self, _from: i32, _to: i32) -> bool {
        true
    }
}

/// Which way rows run down the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    /// The lowest row at the top (tracks).
    TopDown,
    /// The highest row at the top (pitches).
    BottomUp,
}

/// Where each row is: cumulative tops in row units from the layout's top,
/// so "which row is at this y" is a binary search rather than a division
/// by a pitch that variable heights do not have.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowLayout {
    /// Row `i` (counted down the screen) spans `edges[i]..edges[i + 1]`;
    /// `rows + 1` of them.
    edges: Vec<f64>,
    lo: i32,
    orientation: Option<Orientation>,
}

impl RowLayout {
    /// Rows `lo, lo + 1, …` of these heights, top-down.
    #[must_use]
    pub fn from_heights(lo: i32, heights: impl IntoIterator<Item = f64>) -> Self {
        let mut edges = vec![0.0];
        let mut y = 0.0;
        for h in heights {
            y += h;
            edges.push(y);
        }
        Self {
            edges,
            lo,
            orientation: Some(Orientation::TopDown),
        }
    }

    /// A model's rows, in `orientation`.
    #[must_use]
    pub fn build(model: &dyn RowModel, orientation: Orientation) -> Self {
        let (lo, hi) = model.bounds();
        let mut layout = match orientation {
            Orientation::TopDown => Self::from_heights(lo, (lo..=hi).map(|r| model.height(r))),
            Orientation::BottomUp => {
                Self::from_heights(lo, (lo..=hi).rev().map(|r| model.height(r)))
            }
        };
        layout.orientation = Some(orientation);
        layout
    }

    /// `lo..=hi`, every row `height` tall.
    #[must_use]
    pub fn uniform(lo: i32, hi: i32, height: f64, orientation: Orientation) -> Self {
        let n = usize::try_from(i64::from(hi) - i64::from(lo) + 1).unwrap_or(0);
        let mut layout = Self::from_heights(lo, std::iter::repeat_n(height, n));
        layout.orientation = Some(orientation);
        layout
    }

    /// How many rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.edges.len().saturating_sub(1)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The whole height.
    #[must_use]
    pub fn total(&self) -> f64 {
        self.edges.last().copied().unwrap_or(0.0)
    }

    /// A row's place down the screen.
    fn index_of(&self, row: i32) -> Option<usize> {
        let i = usize::try_from(i64::from(row) - i64::from(self.lo)).ok()?;
        (i < self.len()).then(|| match self.orientation {
            Some(Orientation::BottomUp) => self.len() - 1 - i,
            _ => i,
        })
    }

    fn row_of(&self, index: usize) -> i32 {
        let i = match self.orientation {
            Some(Orientation::BottomUp) => self.len() - 1 - index,
            _ => index,
        };
        self.lo.saturating_add(i32::try_from(i).unwrap_or(i32::MAX))
    }

    /// A row's top and height.
    #[must_use]
    pub fn extent(&self, row: i32) -> Option<(f64, f64)> {
        let i = self.index_of(row)?;
        let top = *self.edges.get(i)?;
        Some((top, self.edges.get(i + 1)? - top))
    }

    /// The row at `r` down the layout: the one whose top is the last at
    /// or above it. `None` above the first or at and past the end.
    #[must_use]
    pub fn row_at(&self, r: f64) -> Option<i32> {
        if r < 0.0 || self.is_empty() {
            return None;
        }
        let i = self.edges.partition_point(|&y| y <= r).checked_sub(1)?;
        (i < self.len()).then(|| self.row_of(i))
    }

    /// The places down the screen (not rows) that `top..bottom` touches,
    /// clamped to the layout.
    #[must_use]
    pub fn between(&self, top: f64, bottom: f64) -> std::ops::Range<usize> {
        // `partition_point` gives the first row whose TOP is past the
        // edge; the row before it is the one the edge falls inside.
        let first = self.edges.partition_point(|&y| y <= top).saturating_sub(1);
        let last = self.edges.partition_point(|&y| y < bottom);
        first.min(self.len())..last.min(self.len())
    }
}

/// One lane of a [`stack`]: its top and height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StackLane {
    pub y: f64,
    pub height: f64,
}

/// Lay lanes of these `weights` out down `height`, each at least `floor`
/// tall: every lane its floor first, then the rest shared by weight. When
/// the floors want more room than there is, every lane is laid out at its
/// floor (still in proportion to its weight) and the stack overflows for
/// the caller to scroll; only a view too small for one floored lane splits
/// evenly.
#[must_use]
pub fn stack(weights: &[f64], height: f64, floor: f64) -> Vec<StackLane> {
    if weights.is_empty() || height <= 0.0 {
        return Vec::new();
    }
    let floor = floor.max(0.0);
    #[expect(clippy::cast_precision_loss, reason = "a count of lanes")]
    let n = weights.len() as f64;
    let mut y = 0.0;
    let mut lay = |h: f64| {
        let lane = StackLane { y, height: h };
        y += h;
        lane
    };
    if floor * n > height && floor <= height {
        let total = weights.iter().sum::<f64>().max(1e-6);
        return weights
            .iter()
            .map(|w| lay((floor * n * (w / total)).max(floor)))
            .collect();
    }
    if floor * n >= height {
        return weights.iter().map(|_| lay(height / n)).collect();
    }
    let spare = height - floor * n;
    let total: f64 = weights.iter().sum();
    weights
        .iter()
        .map(|w| lay(floor + spare * (w / total)))
        .collect()
}

/// Which lane `y` falls in.
#[must_use]
pub fn lane_at(lanes: &[StackLane], y: f64) -> Option<usize> {
    lanes.iter().position(|l| y >= l.y && y < l.y + l.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_found_by_their_tops() {
        let layout = RowLayout::from_heights(0, [32.0, 64.0, 10.0]);
        assert_eq!(layout.total(), 106.0);
        assert_eq!(layout.extent(1), Some((32.0, 64.0)));
        assert_eq!(layout.row_at(0.0), Some(0));
        assert_eq!(layout.row_at(32.0), Some(1));
        assert_eq!(layout.row_at(105.9), Some(2));
        assert_eq!(layout.row_at(106.0), None);
        assert_eq!(layout.row_at(-1.0), None);
        assert_eq!(layout.between(40.0, 100.0), 1..3);
        assert_eq!(layout.between(0.0, 1e9), 0..3);
    }

    #[test]
    fn bottom_up_puts_the_highest_row_on_top() {
        let layout = RowLayout::uniform(60, 63, 10.0, Orientation::BottomUp);
        assert_eq!(layout.extent(63), Some((0.0, 10.0)));
        assert_eq!(layout.extent(60), Some((30.0, 10.0)));
        assert_eq!(layout.row_at(5.0), Some(63));
        assert_eq!(layout.row_at(35.0), Some(60));
    }

    #[test]
    fn a_stack_shares_the_spare_by_weight_over_the_floor() {
        let lanes = stack(&[1.0, 3.0], 500.0, 100.0);
        assert_eq!(
            lanes[0],
            StackLane {
                y: 0.0,
                height: 175.0
            }
        );
        assert_eq!(
            lanes[1],
            StackLane {
                y: 175.0,
                height: 325.0
            }
        );
        assert_eq!(lane_at(&lanes, 200.0), Some(1));
        assert_eq!(lane_at(&lanes, 500.0), None);
    }

    #[test]
    fn too_many_lanes_overflow_at_the_floor() {
        let lanes = stack(&[1.0; 20], 1000.0, 120.0);
        assert!(lanes.iter().all(|l| l.height >= 120.0));
        assert!(lanes.last().map_or(0.0, |l| l.y + l.height) > 1000.0);
    }
}
