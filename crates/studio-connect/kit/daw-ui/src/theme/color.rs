//! The colour type the theme is authored in.
//!
//! Hex in the file, RGBA in memory, and whatever each consumer needs on the
//! way out: CSS for the Dioxus panels, a `COLORREF` int for REAPER's
//! `.ReaperTheme`.

use facet::Facet;

/// An RGBA colour, authored as `#rrggbb` or `#rrggbbaa`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Facet)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parse `#rgb`, `#rrggbb` or `#rrggbbaa` (leading `#` optional).
    pub fn hex(s: &str) -> Option<Self> {
        let h = s.trim().trim_start_matches('#');
        let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
        let nib = |i: usize| u8::from_str_radix(&h[i..i + 1], 16).ok().map(|v| v * 17);
        match h.len() {
            3 => Some(Self::rgb(nib(0)?, nib(1)?, nib(2)?)),
            6 => Some(Self::rgb(byte(0)?, byte(2)?, byte(4)?)),
            8 => Some(Self::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => None,
        }
    }

    /// `#rrggbb`, or `#rrggbbaa` when not fully opaque.
    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }

    /// CSS, for the inline styles the panels render with.
    pub fn css(self) -> String {
        if self.a == 255 {
            self.to_hex()
        } else {
            format!(
                "rgba({},{},{},{:.3})",
                self.r,
                self.g,
                self.b,
                self.a as f32 / 255.0
            )
        }
    }

    /// A Windows `COLORREF` (`0x00BBGGRR`) — what `.ReaperTheme` stores.
    ///
    /// Alpha is dropped: REAPER keeps blending in separate `*_drawmode`
    /// words, not in the colour.
    pub fn to_colorref(self) -> i32 {
        ((self.b as i32) << 16) | ((self.g as i32) << 8) | self.r as i32
    }

    /// Blend `self` toward `other` by `t` (0 = self, 1 = other).
    pub fn mix(self, other: Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let c = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Color::rgba(
            c(self.r, other.r),
            c(self.g, other.g),
            c(self.b, other.b),
            c(self.a, other.a),
        )
    }

    pub fn with_alpha(self, a: u8) -> Color {
        Color { a, ..self }
    }

    /// Relative luminance (WCAG), for picking a readable foreground.
    pub fn luminance(self) -> f32 {
        let c = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.039_28 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * c(self.r) + 0.7152 * c(self.g) + 0.0722 * c(self.b)
    }

    /// Lighten (positive) or darken (negative) by `amount` (0–1).
    pub fn shade(self, amount: f32) -> Color {
        let target = if amount >= 0.0 {
            Color::rgb(255, 255, 255)
        } else {
            Color::rgb(0, 0, 0)
        };
        self.mix(target, amount.abs())
    }
}
