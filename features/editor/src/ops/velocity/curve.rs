//! The Bézier ramp — draw a velocity shape and lay it across the notes.
//!
//! MVelocity's best idea. Instead of typing numbers you drag a curve, and
//! the notes in the span take their velocity from its height: a rise for
//! a crescendo, an S for a swell, a fast decay for a drum fill trailing
//! off. The notes are sampled at even parameter steps across the curve,
//! so the shape stretches to fit however many notes you selected.
//!
//! ## Divergence from the Lua: the height *is* the velocity
//!
//! Upstream maps the curve twice. It reads the endpoint heights as a
//! velocity range (`y(0)*127` … `y(1)*127`), then uses `y(t)` a second
//! time as the blend *between* those two — so the curve's own height
//! feeds both the range and the position within it. That's exact only
//! when the curve is anchored at 0 and 1, which every shipped preset is;
//! draw a curve from 0.5 to 0.8 and the first note comes out at 82
//! instead of the 63 the widget is showing you. It also forces the
//! `invert` hack upstream carries — auto-mirroring whenever the curve
//! descends, to undo the double mapping's sign flip.
//!
//! Here `velocity(t) = y(t) * 127`, clamped to the range. One mapping,
//! the widget tells the truth for any curve you can draw, and mirroring
//! goes back to being a thing you ask for ([`Curve::invert`]) rather than
//! something that happens to you. On the anchored presets the two agree
//! exactly, so nothing that worked upstream changes.

use super::{MAX_VELOCITY, Note, Range, VelocityEdit, targets};

/// A control point. Both coordinates are normalized 0.0..=1.0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    /// Horizontal position. Cosmetic: the curve is sampled by parameter,
    /// not by x, so this only shapes how the widget looks and how the
    /// handles land under the mouse. Kept because dragging a handle that
    /// can only move vertically feels broken.
    pub x: f64,
    /// Height, and therefore velocity: 0.0 = velocity 0, 1.0 = 127.
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self {
            x: x.clamp(0.0, 1.0),
            y: y.clamp(0.0, 1.0),
        }
    }
}

/// A Bézier curve over its control points.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    points: Vec<Point>,
}

impl Default for Curve {
    /// A flat line at full velocity — visibly a no-op until you drag it.
    fn default() -> Self {
        Self {
            points: vec![
                Point::new(0.0, 1.0),
                Point::new(0.33, 1.0),
                Point::new(0.66, 1.0),
                Point::new(1.0, 1.0),
            ],
        }
    }
}

impl Curve {
    /// Build a curve from control points.
    ///
    /// Fewer than two points can't describe a ramp, so those fall back to
    /// [`Curve::default`].
    pub fn new(points: impl IntoIterator<Item = Point>) -> Self {
        let points: Vec<Point> = points.into_iter().collect();
        if points.len() < 2 {
            Self::default()
        } else {
            Self { points }
        }
    }

    pub fn points(&self) -> &[Point] {
        &self.points
    }

    /// Move control point `i`. Out-of-bounds `i` is ignored.
    pub fn set_point(&mut self, i: usize, point: Point) {
        if let Some(p) = self.points.get_mut(i) {
            *p = point;
        }
    }

    /// Mirror the curve vertically — a crescendo becomes a diminuendo.
    pub fn invert(&mut self) {
        for p in &mut self.points {
            p.y = 1.0 - p.y;
        }
    }

    /// Evaluate the curve at `t` in 0.0..=1.0, returning `(x, y)`.
    ///
    /// A Bézier of degree `points.len() - 1`, evaluated with the
    /// Bernstein basis. Computed iteratively rather than through
    /// factorials: upstream's `comb` builds `n!` afresh per point per
    /// note, which starts losing f64 precision around a dozen control
    /// points and overflows outright not far past that.
    pub fn evaluate(&self, t: f64) -> (f64, f64) {
        let t = t.clamp(0.0, 1.0);
        let n = self.points.len() - 1;
        let (mut x, mut y) = (0.0, 0.0);
        // Binomial coefficient carried forward: C(n,k) = C(n,k-1)*(n-k+1)/k.
        let mut binomial = 1.0_f64;
        for (k, p) in self.points.iter().enumerate() {
            if k > 0 {
                binomial = binomial * ((n - k + 1) as f64) / (k as f64);
            }
            let basis = binomial * t.powi(k as i32) * (1.0 - t).powi((n - k) as i32);
            x += basis * p.x;
            y += basis * p.y;
        }
        (x, y)
    }

    /// The velocity this curve gives at parameter `t`, before clamping.
    pub fn velocity_at(&self, t: f64) -> f64 {
        self.evaluate(t).1 * f64::from(MAX_VELOCITY)
    }

    /// Lay the curve across `notes`, sampling it at even steps.
    ///
    /// Unlike the other three engines this one *replaces* velocities
    /// rather than blending from the baseline — a drawn ramp is an
    /// absolute statement about what the phrase should do, and blending
    /// it would mean you can never actually get the shape you drew. It
    /// stays undoable through [`super::Session`], which holds the
    /// baseline regardless.
    pub fn apply(&self, notes: &[Note], range: Range) -> Vec<VelocityEdit> {
        self.apply_blended(notes, 1.0, range)
    }

    /// As [`Curve::apply`], mixed `amount` of the way in.
    ///
    /// `1.0` is [`Curve::apply`] — the curve stated absolutely, which is
    /// what the drawn contour means. Below that it is a *blend* toward
    /// the shape from whatever the notes already are, so a ramp can be
    /// dialled from "a hint of a crescendo" to "exactly this contour"
    /// without redrawing it.
    ///
    /// Which is the difference between a preset and a gesture. A preset
    /// applied at full strength flattens whatever performance was there;
    /// the same preset at 30% *leans* on it, and the accents you played
    /// survive underneath. `pattern` and `randomize` already take an
    /// amount for the same reason — this makes the curve the third.
    pub fn apply_blended(&self, notes: &[Note], amount: f64, range: Range) -> Vec<VelocityEdit> {
        let amount = amount.clamp(0.0, 1.0);
        let picked: Vec<(u32, u8)> = targets(notes).map(|(_, n)| (n.index, n.velocity)).collect();
        let last = picked.len().saturating_sub(1);
        picked
            .into_iter()
            .enumerate()
            .map(|(i, (index, was))| {
                // A single note sits at the curve's start, not at a
                // 0/0 division.
                let t = if last == 0 {
                    0.0
                } else {
                    i as f64 / last as f64
                };
                let target = self.velocity_at(t);
                VelocityEdit {
                    index,
                    velocity: range.clamp(was as f64 + (target - was as f64) * amount),
                }
            })
            .collect()
    }
}

/// MVelocity's ten shipped curve shapes, named for what they do.
///
/// Upstream calls them `preset1`..`preset10`, which tells you nothing at
/// the call site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurvePreset {
    /// Near-linear crescendo.
    Rise,
    /// Near-linear diminuendo.
    Fall,
    /// Smooth ease-in-out crescendo.
    RiseSmooth,
    /// Smooth ease-in-out diminuendo.
    FallSmooth,
    /// Overshooting S — dips before it climbs.
    RiseS,
    /// Overshooting S — lifts before it drops.
    FallS,
    /// Jumps early, then flattens near the top.
    RiseFast,
    /// Stays low, then climbs sharply at the end.
    RiseSlow,
    /// Drops sharply, then trails along the floor.
    FallFast,
    /// Holds up, then drops sharply at the end.
    FallSlow,
}

impl CurvePreset {
    /// Every preset, in the order a UI should list them: rises, then
    /// falls, gentlest first.
    pub const ALL: [CurvePreset; 10] = [
        CurvePreset::Rise,
        CurvePreset::RiseSmooth,
        CurvePreset::RiseS,
        CurvePreset::RiseFast,
        CurvePreset::RiseSlow,
        CurvePreset::Fall,
        CurvePreset::FallSmooth,
        CurvePreset::FallS,
        CurvePreset::FallFast,
        CurvePreset::FallSlow,
    ];

    /// A short label for a button.
    pub fn label(self) -> &'static str {
        match self {
            CurvePreset::Rise => "Rise",
            CurvePreset::Fall => "Fall",
            CurvePreset::RiseSmooth => "Rise S-curve",
            CurvePreset::FallSmooth => "Fall S-curve",
            CurvePreset::RiseS => "Rise dip",
            CurvePreset::FallS => "Fall lift",
            CurvePreset::RiseFast => "Rise fast",
            CurvePreset::RiseSlow => "Rise late",
            CurvePreset::FallFast => "Fall fast",
            CurvePreset::FallSlow => "Fall late",
        }
    }

    /// The control points, straight from MVelocity's `apply_preset`.
    pub fn curve(self) -> Curve {
        let p = |pts: [(f64, f64); 4]| Curve::new(pts.map(|(x, y)| Point::new(x, y)));
        match self {
            CurvePreset::Rise => p([(0.01, 0.0), (0.33, 0.30), (0.69, 0.63), (0.99, 1.0)]),
            CurvePreset::Fall => p([(0.01, 1.0), (0.33, 0.63), (0.59, 0.36), (0.99, 0.0)]),
            CurvePreset::FallS => p([(0.01, 1.0), (0.0, 0.0), (1.0, 1.0), (0.99, 0.0)]),
            CurvePreset::RiseS => p([(0.01, 0.0), (0.0, 1.0), (1.0, 0.0), (0.99, 1.0)]),
            CurvePreset::FallSmooth => p([(0.01, 1.0), (1.0, 1.0), (0.0, 0.0), (0.99, 0.0)]),
            CurvePreset::RiseSmooth => p([(0.01, 0.0), (1.0, 0.0), (0.0, 1.0), (0.99, 1.0)]),
            CurvePreset::FallFast => p([(0.01, 1.0), (0.0, 0.0), (0.0, 0.0), (0.99, 0.0)]),
            CurvePreset::RiseSlow => p([(0.01, 0.0), (1.0, 0.0), (1.0, 0.0), (0.99, 1.0)]),
            CurvePreset::RiseFast => p([(0.01, 0.0), (0.0, 1.0), (0.0, 1.0), (0.99, 1.0)]),
            CurvePreset::FallSlow => p([(0.01, 1.0), (1.0, 1.0), (1.0, 1.0), (0.99, 0.0)]),
        }
    }
}
