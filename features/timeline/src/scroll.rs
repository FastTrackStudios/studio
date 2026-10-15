//! Scrolling a timeline by hand and by itself: a fling that carries on
//! after a finger lets go, the finger's speed it is thrown at, a pinch's
//! step, the page turn that keeps the play cursor in view, and the edge
//! scroll that carries a drag past the edge of the lanes.
//!
//! Screen px throughout; time in milliseconds where it says so.

/// A scroll still moving after the finger let go: the speed it left at,
/// dying away as a thrown list does on a phone.
#[derive(Clone, Copy, Debug)]
pub struct Fling {
    /// CSS pixels a millisecond, on each axis, in the scroll's direction.
    pub velocity: (f64, f64),
    at: web_time::Instant,
}

/// How fast a fling dies: the time for its speed to fall to a third,
/// near enough what iOS's own scroll views take.
const FLING_DECAY_MS: f64 = 325.0;
/// Below this (CSS pixels a millisecond) a fling has stopped.
const FLING_REST: f64 = 0.02;
/// The fastest a fling starts, however hard the flick.
const FLING_MAX: f64 = 6.0;

impl Fling {
    /// A fling at `velocity` (CSS pixels a millisecond, in the scroll's
    /// direction), or none if that is too slow to be one.
    #[must_use]
    pub fn thrown(velocity: (f64, f64)) -> Option<Self> {
        let velocity = (
            velocity.0.clamp(-FLING_MAX, FLING_MAX),
            velocity.1.clamp(-FLING_MAX, FLING_MAX),
        );
        (velocity.0.hypot(velocity.1) > FLING_REST * 5.0).then(|| Self {
            velocity,
            at: web_time::Instant::now(),
        })
    }

    /// How far it has carried since it was last asked, and whether it is
    /// still going.
    pub fn step(&mut self) -> ((f64, f64), bool) {
        let now = web_time::Instant::now();
        let dt = now.duration_since(self.at).as_secs_f64() * 1000.0;
        self.at = now;
        // The distance under an exponentially dying speed over `dt`.
        let decay = (-dt / FLING_DECAY_MS).exp();
        let carried = FLING_DECAY_MS * (1.0 - decay);
        let moved = (self.velocity.0 * carried, self.velocity.1 * carried);
        self.velocity = (self.velocity.0 * decay, self.velocity.1 * decay);
        (moved, self.velocity.0.hypot(self.velocity.1) > FLING_REST)
    }
}

/// A finger's speed, from its last few moves: CSS pixels a millisecond on
/// each axis. Kept short, so a finger that slowed before letting go
/// throws gently.
#[derive(Clone, Debug, Default)]
pub struct Speed {
    samples: Vec<(web_time::Instant, (f64, f64))>,
}

impl Speed {
    /// The finger is at `at`, now.
    pub fn moved(&mut self, at: (f64, f64)) {
        let now = web_time::Instant::now();
        self.samples.push((now, at));
        self.samples
            .retain(|(when, _)| now.duration_since(*when).as_millis() <= 100);
    }

    /// Its speed over the samples kept.
    #[must_use]
    pub fn velocity(&self) -> (f64, f64) {
        let (Some(first), Some(last)) = (self.samples.first(), self.samples.last()) else {
            return (0.0, 0.0);
        };
        let ms = last.0.duration_since(first.0).as_secs_f64() * 1000.0;
        if ms < 1.0 {
            return (0.0, 0.0);
        }
        ((last.1.0 - first.1.0) / ms, (last.1.1 - first.1.1) / ms)
    }
}

/// One step of a two-finger gesture: how far the fingers spread on each
/// axis since the last (`sx` across, `sy` down — 1.0 is no change), where
/// they are now (`mid`), and how far that moved (`dx`, `dy`). What any
/// view that pinches and pans does with it is its own: a timeline zooms
/// time across and rows down; a page zooms both alike.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub sx: f64,
    pub sy: f64,
    pub mid: (f64, f64),
    pub dx: f64,
    pub dy: f64,
}

/// How far apart two fingers must be on an axis for a pinch to spread it:
/// nearer than this they are side by side, not apart, and that axis's zoom
/// stays.
pub const PINCH_SPAN: f64 = 40.0;

/// The step between two fingers at `a0`, `b0` and the same two at `a1`,
/// `b1`. Each axis's spread is held to a quarter either way per step, so a
/// finger that jumps (a dropped event) does not throw the zoom.
#[must_use]
pub fn pinch_step(a0: (f64, f64), b0: (f64, f64), a1: (f64, f64), b1: (f64, f64)) -> Step {
    let spread = |old: f64, new: f64| {
        if old.abs() < PINCH_SPAN || new.abs() < 1.0 {
            1.0
        } else {
            (new.abs() / old.abs()).clamp(0.8, 1.25)
        }
    };
    let mid0 = ((a0.0 + b0.0) / 2.0, (a0.1 + b0.1) / 2.0);
    let mid1 = ((a1.0 + b1.0) / 2.0, (a1.1 + b1.1) / 2.0);
    Step {
        sx: spread(a0.0 - b0.0, a1.0 - b1.0),
        sy: spread(a0.1 - b0.1, a1.1 - b1.1),
        mid: mid1,
        dx: mid1.0 - mid0.0,
        dy: mid1.1 - mid0.1,
    }
}

/// Where to page the view to keep `x` (the play cursor, in lane pixels)
/// in sight, or `None` while it is. The cursor lands a twentieth of the
/// view in from the left; the view turns just before the cursor reaches
/// the right edge, and at once when it is behind the left one.
#[must_use]
pub fn page_to(x: f64, scroll: f64, width: f64, travel: f64) -> Option<f64> {
    let lead = width * 0.05;
    let visible = x >= scroll && x <= scroll + width - lead;
    (!visible).then(|| (x - lead).clamp(0.0, travel.max(0.0)))
}

/// How near an edge of the lanes (px) a held drag starts to scroll them.
pub const EDGE_MARGIN: f64 = 24.0;

/// How fast (px a second) a held drag scrolls the lanes at the very edge
/// and beyond.
pub const EDGE_SPEED: f64 = 900.0;

/// The scroll a drag held at `at` (px from the lanes' origin) asks of a
/// view `size` big, in px a second on each axis: none inside the
/// margins, rising to [`EDGE_SPEED`] at the edge, and held there past it.
/// The REAPER feel: the nearer the edge, the faster.
#[must_use]
pub fn edge_scroll(at: (f64, f64), size: (f64, f64)) -> (f64, f64) {
    let axis = |p: f64, len: f64| {
        if len <= EDGE_MARGIN * 2.0 {
            return 0.0;
        }
        let into = |d: f64| ((EDGE_MARGIN - d) / EDGE_MARGIN).clamp(0.0, 1.0);
        if p < EDGE_MARGIN {
            -EDGE_SPEED * into(p)
        } else if p > len - EDGE_MARGIN {
            EDGE_SPEED * into(len - p)
        } else {
            0.0
        }
    };
    (axis(at.0, size.0), axis(at.1, size.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pinch_spreads_each_axis_on_its_own_and_carries_the_middle() {
        // Apart across only: time zooms, rows do not.
        let step = pinch_step(
            (100.0, 200.0),
            (200.0, 200.0),
            (80.0, 200.0),
            (220.0, 200.0),
        );
        assert!((step.sx - 1.25).abs() < 1e-9, "{step:?}");
        assert!((step.sy - 1.0).abs() < 1e-9);
        assert_eq!((step.dx, step.dy), (0.0, 0.0), "spread about its middle");
        // Both fingers moved together: a drag, no zoom.
        let step = pinch_step(
            (100.0, 100.0),
            (200.0, 200.0),
            (110.0, 90.0),
            (210.0, 190.0),
        );
        assert_eq!((step.sx, step.sy), (1.0, 1.0));
        assert_eq!((step.dx, step.dy), (10.0, -10.0));
    }

    #[test]
    fn the_view_pages_rather_than_slides() {
        // In view: nothing.
        assert_eq!(page_to(500.0, 0.0, 1000.0, 10_000.0), None);
        // At the right edge: a page on, the cursor near the left.
        assert_eq!(page_to(960.0, 0.0, 1000.0, 10_000.0), Some(910.0));
        // Jumped behind the view: back to it.
        assert_eq!(page_to(100.0, 5000.0, 1000.0, 10_000.0), Some(50.0));
        // Never past the end, never before the start.
        assert_eq!(page_to(9_990.0, 0.0, 1000.0, 9_500.0), Some(9_500.0));
        assert_eq!(page_to(10.0, 500.0, 1000.0, 10_000.0), Some(0.0));
    }

    #[test]
    fn a_drag_held_at_the_edge_scrolls_and_one_inside_does_not() {
        let size = (1000.0, 600.0);
        assert_eq!(edge_scroll((500.0, 300.0), size), (0.0, 0.0));
        let (dx, dy) = edge_scroll((995.0, 300.0), size);
        assert!(dx > 0.0 && dy == 0.0, "{dx} {dy}");
        assert_eq!(
            edge_scroll((1200.0, -50.0), size),
            (EDGE_SPEED, -EDGE_SPEED)
        );
        // Nearer the edge, faster.
        assert!(edge_scroll((990.0, 300.0), size).0 > edge_scroll((980.0, 300.0), size).0);
    }
}
