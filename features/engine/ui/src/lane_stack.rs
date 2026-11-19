//! The lanes a row showing its lanes stacks, top to bottom, and how tall
//! each is.
//!
//! The lane that plays comes first, at the row's own height, then the
//! comp (when another lane plays), then every take at [`LANE_H`] — so the
//! row grows with its takes rather than squeezing them into slivers, and
//! what is heard is always at its top: the lane a scroll down through the
//! takes keeps pinned ([`Stack::pinned`]).
//!
//! ```ignore
//! let stack = Stack::new(&kit, items.iter().map(|i| i.fixed_lane.unwrap_or(0)), 48.0);
//! let (top, bottom) = stack.band(lane);   // fractions of the row
//! let lane = stack.lane_at(0.4);
//! ```

/// How tall a lane under the head is, in session units.
pub const LANE_H: f64 = 22.0;

/// A row's lanes, top to bottom, and where each starts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stack {
    /// The lanes with a strip, in the order they are stacked.
    lanes: Vec<u32>,
    /// Where each strip starts, from the row's top, and the row's height
    /// last: one more than `lanes`.
    tops: Vec<f64>,
}

impl Stack {
    /// `track`'s lanes holding something (`held`, each item's lane) and
    /// the one that plays, stacked: the playing lane first at `head`
    /// tall, the comp next when it is not the one playing, then the rest
    /// in lane order at [`LANE_H`].
    #[must_use]
    pub fn new(track: &daw_proto::Track, held: impl IntoIterator<Item = u32>, head: f64) -> Self {
        let playing = crate::lanes::playing(track);
        let comp = (0..track.lane_count.min(64)).find(|&lane| {
            crate::lanes::name(track, lane).is_some_and(|n| n.eq_ignore_ascii_case("COMP"))
        });
        let mut rest: Vec<u32> = held.into_iter().chain(comp).collect();
        rest.sort_unstable();
        rest.dedup();
        let comp = comp.filter(|c| Some(*c) != playing);
        let first: Vec<u32> = playing.into_iter().chain(comp).collect();
        let mut lanes = first.clone();
        lanes.extend(rest.into_iter().filter(|l| !first.contains(l)));
        if lanes.is_empty() {
            lanes.push(0);
        }
        let tops = std::iter::once(0.0)
            .chain(lanes.iter().enumerate().scan(0.0, |at, (i, _)| {
                *at += if i == 0 { head.max(LANE_H) } else { LANE_H };
                Some(*at)
            }))
            .collect();
        Self { lanes, tops }
    }

    /// The lanes with a strip, top to bottom; empty for a row showing
    /// what plays.
    #[must_use]
    pub fn lanes(&self) -> &[u32] {
        &self.lanes
    }

    /// Whether the row shows what plays rather than its lanes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lanes.is_empty()
    }

    /// How tall the whole stack is, in session units.
    #[must_use]
    pub fn height(&self) -> f64 {
        self.tops.last().copied().unwrap_or(0.0)
    }

    /// How tall the strip kept in sight is: the top one, the lane that
    /// plays.
    #[must_use]
    pub fn pinned(&self) -> f64 {
        self.tops.get(1).copied().unwrap_or(0.0)
    }

    /// Lane `lane`'s strip, as fractions of the row from its top; the
    /// last strip for a lane it gives none.
    #[must_use]
    pub fn band(&self, lane: u32) -> (f64, f64) {
        let strip = self
            .lanes
            .iter()
            .position(|l| *l == lane)
            .unwrap_or_else(|| self.lanes.len().saturating_sub(1));
        let height = self.height();
        if height <= 0.0 {
            return (0.0, 1.0);
        }
        let at = |i: usize| self.tops.get(i).copied().unwrap_or(height) / height;
        (at(strip), at(strip.saturating_add(1)))
    }

    /// The lane at `fraction` of the row from its top; `None` off it.
    #[must_use]
    pub fn lane_at(&self, fraction: f64) -> Option<u32> {
        if !(0.0..1.0).contains(&fraction) {
            return None;
        }
        let y = fraction * self.height();
        let strip = self.tops.partition_point(|top| *top <= y).checked_sub(1)?;
        self.lanes.get(strip).copied()
    }

    /// Where each strip but the first starts, as fractions of the row:
    /// the lines between them.
    pub fn seams(&self) -> impl Iterator<Item = f64> + '_ {
        let height = self.height().max(f64::MIN_POSITIVE);
        self.tops
            .get(1..self.lanes.len())
            .unwrap_or(&[])
            .iter()
            .map(move |top| top / height)
    }
}

/// Where a row whose stack keeps `pinned` in sight is drawn pinned, with
/// the view's top at `view_top` (session units): how far its head moves
/// down to stay there. `None` while its own top is in sight, or once the
/// rest of it has scrolled up past the head.
#[must_use]
pub fn pinned_by(top: f64, height: f64, pinned: f64, view_top: f64) -> Option<f64> {
    let by = view_top - top;
    (pinned > 0.0 && by > 0.0 && by < height - pinned).then_some(by)
}
