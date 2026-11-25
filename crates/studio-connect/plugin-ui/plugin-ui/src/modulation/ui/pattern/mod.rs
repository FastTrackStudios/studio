//! Portable geometry + SVG-path helpers for pattern (MSEG) editors.
//!
//! Framework-free (no Dioxus): everything here maps between the
//! normalized pattern space (`x`, `y` ∈ 0..1, y = 1 at the TOP of the
//! value range) and a pixel viewBox, and renders
//! [`dsp_core::modulation::Pattern`] curves as SVG path strings. The Dioxus
//! component in `signal-ui` (and any future host: plugin editors, the
//! web remote, story previews) wires pointer events to these helpers.
//!
//! Mirrors the `eq-ui` split: interaction math lives here, wasm-clean
//! and unit-tested; the component stays a thin event shim.

use std::fmt::Write;

use dsp_core::modulation::{CurveType, Pattern, Point};

/// Maps normalized pattern coordinates to a pixel viewBox and back.
///
/// Pattern `y = 1.0` renders at the top (`py = 0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PatternMapper {
    pub width: f64,
    pub height: f64,
    /// Inset in px so edge points stay grabbable.
    pub pad: f64,
}

impl PatternMapper {
    #[must_use]
    pub const fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            pad: 6.0,
        }
    }

    #[inline]
    #[must_use]
    pub fn x_to_px(&self, x: f64) -> f64 {
        x.clamp(0.0, 1.0)
            .mul_add(2.0f64.mul_add(-self.pad, self.width), self.pad)
    }

    #[inline]
    #[must_use]
    pub fn y_to_px(&self, y: f64) -> f64 {
        (1.0 - y.clamp(0.0, 1.0)).mul_add(2.0f64.mul_add(-self.pad, self.height), self.pad)
    }

    #[inline]
    #[must_use]
    pub fn px_to_x(&self, px: f64) -> f64 {
        ((px - self.pad) / 2.0f64.mul_add(-self.pad, self.width)).clamp(0.0, 1.0)
    }

    #[inline]
    #[must_use]
    pub fn px_to_y(&self, py: f64) -> f64 {
        (1.0 - (py - self.pad) / 2.0f64.mul_add(-self.pad, self.height)).clamp(0.0, 1.0)
    }
}

/// Index of the point nearest to pixel `(px, py)` within `radius_px`,
/// or `None`. Points are the editable handles, so hit-testing runs in
/// pixel space (a fixed grab radius regardless of zoom).
#[must_use]
pub fn nearest_point(
    points: &[Point],
    mapper: &PatternMapper,
    px: f64,
    py: f64,
    radius_px: f64,
) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (i, p) in points.iter().enumerate() {
        let dx = mapper.x_to_px(p.x) - px;
        let dy = mapper.y_to_px(p.y) - py;
        let d2 = dx.mul_add(dx, dy * dy);
        if d2 <= radius_px * radius_px && best.is_none_or(|(_, bd)| d2 < bd) {
            best = Some((i, d2));
        }
    }
    best.map(|(i, _)| i)
}

/// Sample the pattern into an SVG stroke path (`M … L …`) and a closed
/// fill path (down to the bottom edge). `samples` ≥ 2; 128–256 renders
/// smoothly at typical widths.
#[must_use]
pub fn pattern_paths(
    pattern: &Pattern,
    mapper: &PatternMapper,
    samples: usize,
) -> (String, String) {
    let n = samples.max(2);
    let mut stroke = String::with_capacity(n * 14 + 8);
    for i in 0..n {
        let x = i as f64 / (n - 1) as f64;
        let px = mapper.x_to_px(x);
        let py = mapper.y_to_px(pattern.get_y(x));
        if i == 0 {
            let _ = write!(stroke, "M{px:.1} {py:.1}");
        } else {
            let _ = write!(stroke, " L{px:.1} {py:.1}");
        }
    }
    let bottom = mapper.y_to_px(0.0);
    let left = mapper.x_to_px(0.0);
    let right = mapper.x_to_px(1.0);
    let fill = format!("{stroke} L{right:.1} {bottom:.1} L{left:.1} {bottom:.1} Z");
    (stroke, fill)
}

/// Drag update: clamp a moved point into range and keep it between its
/// x-neighbors (matching how MSEG editors constrain reorder). First and
/// last points are pinned to x = 0 / x = 1.
#[must_use]
pub fn constrained_move(points: &[Point], index: usize, x: f64, y: f64) -> (f64, f64) {
    let n = points.len();
    let eps = 1.0e-4;
    let x = if index == 0 {
        0.0
    } else if index + 1 == n {
        1.0
    } else {
        let lo = points[index - 1].x + eps;
        let hi = points[index + 1].x - eps;
        x.clamp(lo.min(hi), hi.max(lo))
    };
    (x, y.clamp(0.0, 1.0))
}

/// Cycle a point's curve type through the 9 variants (right-click /
/// modifier-click gesture in the editor).
#[must_use]
pub fn next_curve_type(current: CurveType) -> CurveType {
    CurveType::from_u8((current as u8 + 1) % 9)
}

/// Scroll-wheel tension adjust: accumulate wheel delta into −1..1.
#[must_use]
pub const fn adjust_tension(current: f64, wheel_delta: f64) -> f64 {
    wheel_delta.mul_add(0.05, current).clamp(-1.0, 1.0)
}

/// Build a `Pattern` evaluator from any point slice (the component
/// keeps its working copy as plain points).
#[must_use]
pub fn build_pattern(points: &[Point], tension_mult: f64) -> Pattern {
    let mut pat = Pattern::new();
    pat.set_points(points.to_vec());
    pat.tension_mult = tension_mult;
    pat
}
