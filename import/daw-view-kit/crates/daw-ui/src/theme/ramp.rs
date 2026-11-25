//! A luminance → colour ramp for restyling neutral theme artwork.
//!
//! REAPER's chrome — toolbar backgrounds, mixer strips, button faces, the
//! TCP frame — is **PNG art, not palette keys**, so no amount of colour
//! work reaches it. That is why a fully-mapped palette still leaves the
//! mixer looking like the theme it was forked from.
//!
//! The art is almost entirely neutral grey at various lightnesses, which
//! makes it remappable: read each pixel's luminance, look it up on a ramp
//! built from the theme, and the whole chrome moves onto the theme's
//! surfaces while every bevel, gradient and antialiased edge survives
//! intact — because their *relative* lightness is preserved.
//!
//! Coloured pixels pass through untouched (see [`Ramp::saturation_guard`])
//! so LEDs, record buttons and fader caps keep their meaning. This is the
//! exact complement of `fts_themer::recolor::Retint`, which touches only
//! the saturated pixels.

use crate::theme::color::Color;
use crate::theme::palette::{Chrome, Theme};

/// A luminance ramp plus the rule for what it refuses to touch.
#[derive(Clone, Debug, PartialEq)]
pub struct Ramp {
    /// `(luminance, colour)` stops, ascending. Values between stops are
    /// interpolated.
    pub stops: Vec<(f32, Color)>,
    /// Saturation above which a pixel counts as a coloured element and is
    /// passed through unchanged.
    pub saturation_guard: f32,
}

impl Ramp {
    /// The ramp that maps dark-DAW chrome art onto this theme.
    ///
    /// Stops bunch at the dark end because that is where chrome art
    /// actually lives — a mixer strip spans roughly 0.01–0.15 luminance,
    /// and evenly-spaced stops would flatten all of it onto one value,
    /// erasing exactly the bevels this is trying to preserve.
    pub fn for_chrome(theme: &Theme) -> Self {
        let c = &theme.chrome;
        let mut colours = [
            c.surface_deep(),
            c.surface,
            c.surface_raised,
            c.border,
            c.text_faint,
            c.text_dim,
            c.text,
            c.selected,
        ];
        // Sorted by lightness, because the *palette* need not be.
        //
        // These names ascend in the FTS palette, so listing them in order
        // was enough. They do not ascend everywhere: ReaperTips' border is
        // `col_main_3dsh`, a shadow at #323232, which is darker than its
        // #424242 surfaces — and a ramp whose stops dip maps two different
        // inputs to the same output and inverts the shading between them.
        // Sorting keeps every authored colour in the ramp while making
        // monotonicity a property of the construction rather than a
        // coincidence of the names.
        colours.sort_by(|a, b| a.luminance().total_cmp(&b.luminance()));

        // Positions bunch at the dark end because that is where chrome art
        // actually lives — a mixer strip spans roughly 0.01–0.15 luminance,
        // and evenly-spaced stops would flatten all of it onto one value,
        // erasing exactly the bevels this is trying to preserve.
        let at = [0.000, 0.010, 0.035, 0.080, 0.200, 0.400, 0.700, 1.000];
        Self {
            stops: at.into_iter().zip(colours).collect(),
            saturation_guard: 0.18,
        }
    }

    /// Map one colour.
    pub fn apply(&self, c: Color) -> Color {
        if is_coloured(c, self.saturation_guard) {
            return c;
        }
        self.sample(c.luminance())
    }

    /// The ramp value at `l`, interpolating between stops and clamping
    /// outside them.
    pub fn sample(&self, l: f32) -> Color {
        match self.stops.as_slice() {
            [] => Color::rgb(0, 0, 0),
            [(_, only)] => *only,
            stops => {
                if l <= stops[0].0 {
                    return stops[0].1;
                }
                for pair in stops.windows(2) {
                    let ((l0, c0), (l1, c1)) = (pair[0], pair[1]);
                    if l <= l1 {
                        let span = l1 - l0;
                        let t = if span.abs() < f32::EPSILON {
                            0.0
                        } else {
                            (l - l0) / span
                        };
                        return c0.mix(c1, t);
                    }
                }
                stops[stops.len() - 1].1
            }
        }
    }
}

impl Chrome {
    /// The deepest surface step — a floor for [`Ramp`] without widening the
    /// authored struct.
    pub fn surface_deep(&self) -> Color {
        self.surface.shade(-0.35)
    }
}

/// Is this pixel a coloured element rather than neutral chrome?
///
/// Saturation alone is not enough. HSL saturation is a *ratio*, so it blows
/// up at the ends of the range: `#010203` — three units apart, visually
/// black — computes as 50% saturated, and a naive guard would refuse to
/// remap it. Chrome art is full of such near-black and near-white pixels,
/// so guarding on saturation alone silently skips the darkest chrome, which
/// is most of it.
///
/// So the guard only applies in the middle of the lightness range, where
/// saturation actually means something.
fn is_coloured(c: Color, guard: f32) -> bool {
    let l = lightness(c);
    if !(0.06..=0.94).contains(&l) {
        return false;
    }
    saturation(c) > guard
}

/// HSL lightness, 0–1.
fn lightness(c: Color) -> f32 {
    let (r, g, b) = (c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0);
    (r.max(g).max(b) + r.min(g).min(b)) / 2.0
}

/// HSL saturation of a colour, 0–1.
fn saturation(c: Color) -> f32 {
    let (r, g, b) = (c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    if d.abs() < f32::EPSILON {
        return 0.0;
    }
    let l = (max + min) / 2.0;
    if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    }
}
