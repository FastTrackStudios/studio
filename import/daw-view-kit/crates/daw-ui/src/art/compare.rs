//! Measuring how closely a component reproduces the art it replaces.
//!
//! "Match the original exactly" is not reviewable by eye across hundreds of
//! images — at the sizes REAPER uses (`mcp_bg` is 4×4) the differences that
//! matter are invisible until they are composited into a mixer. So fidelity
//! is a number, and the number is checkable in a test.
//!
//! # What is compared, and what isn't
//!
//! - **Marker pixels are excluded.** They are geometry, copied verbatim, so
//!   scoring them would only inflate the result.
//! - **Colour is compared in *relative* terms.** The goal is not to
//!   reproduce Reapertips' greys — the whole point is to redraw them in our
//!   palette — so an absolute pixel diff would score a correct result as
//!   completely wrong. [`shape_score`] compares the *structure*: which
//!   pixels are opaque, and how each pixel's lightness relates to the
//!   image's own range.
//!
//! That distinction is the crux. A component that gets the silhouette and
//! the internal light/dark relationships right is a faithful replacement in
//! a different colourway; one that matches absolute RGB is just a copy.

use image::RgbaImage;

use crate::art::derive::MARKERS;

/// How closely two images agree, 0–1.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Fidelity {
    /// Agreement on which pixels are drawn at all, 0–1. This is the
    /// silhouette: rounded corners, insets, where a groove sits.
    pub shape: f32,
    /// Agreement on internal light/dark structure, 0–1. This is the
    /// modelling: bevels, ribbing, gradients.
    pub structure: f32,
    /// Pixels compared (excludes markers).
    pub compared: usize,
}

impl Fidelity {
    /// A single number for ranking, weighted toward silhouette — a shape
    /// that is wrong cannot be rescued by good shading.
    pub fn score(self) -> f32 {
        self.shape * 0.6 + self.structure * 0.4
    }
}

/// Is this a WALTER guide rather than art?
fn is_marker(px: [u8; 4]) -> bool {
    MARKERS.contains(&px)
}

/// Perceptual lightness, 0–1.
fn lightness(px: [u8; 4]) -> f32 {
    (0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32) / 255.0
}

/// Compare a rendered image against the original it replaces.
///
/// Returns `None` when the two differ in size — that is a spec error, not a
/// fidelity result, and scoring it would hide the real problem.
pub fn compare(rendered: &RgbaImage, original: &RgbaImage) -> Option<Fidelity> {
    if rendered.dimensions() != original.dimensions() {
        return None;
    }

    let mut compared = 0usize;
    let mut shape_hits = 0usize;

    // Lightness is compared *within each image's own range*, so a dark
    // redraw of a light original still scores well when the internal
    // relationships hold. Collect the ranges first.
    let mut pairs: Vec<(f32, f32)> = Vec::new();
    let (mut rmin, mut rmax) = (f32::MAX, f32::MIN);
    let (mut omin, mut omax) = (f32::MAX, f32::MIN);

    for (r, o) in rendered.pixels().zip(original.pixels()) {
        if is_marker(o.0) {
            continue;
        }
        compared += 1;

        // Silhouette: both drawn, or both not.
        let (ra, oa) = (r.0[3] > 8, o.0[3] > 8);
        if ra == oa {
            shape_hits += 1;
        }
        if ra && oa {
            let (rl, ol) = (lightness(r.0), lightness(o.0));
            rmin = rmin.min(rl);
            rmax = rmax.max(rl);
            omin = omin.min(ol);
            omax = omax.max(ol);
            pairs.push((rl, ol));
        }
    }

    if compared == 0 {
        return Some(Fidelity {
            shape: 1.0,
            structure: 1.0,
            compared: 0,
        });
    }

    let norm = |v: f32, lo: f32, hi: f32| {
        let span = hi - lo;
        if span.abs() < 1e-4 {
            0.5
        } else {
            (v - lo) / span
        }
    };
    let structure = if pairs.is_empty() {
        1.0
    } else {
        let err: f32 = pairs
            .iter()
            .map(|&(r, o)| (norm(r, rmin, rmax) - norm(o, omin, omax)).abs())
            .sum::<f32>()
            / pairs.len() as f32;
        (1.0 - err).clamp(0.0, 1.0)
    };

    Some(Fidelity {
        shape: shape_hits as f32 / compared as f32,
        structure,
        compared,
    })
}
