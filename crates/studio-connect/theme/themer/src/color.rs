//! Color conversion between the three representations this crate juggles.
//!
//! - **`COLORREF`** — what `.ReaperTheme` stores: a *signed* decimal int whose
//!   low three bytes are `0x00BBGGRR` (red in the low byte, not RGB order).
//!   The top byte is not always zero: REAPER packs flag bits there (an unset
//!   key reads back as a large negative number), so writes must preserve it.
//! - **hex** — `#rrggbb`, what a GUI color input speaks.
//! - **HSL** — what recoloring and accent generation work in.

use anyhow::{Result, bail};
use palette::{FromColor, Hsl, IntoColor, Srgb};

/// An opaque 8-bit RGB triple. Alpha lives outside the palette (REAPER
/// encodes it in separate `*_drawmode` words, not in the COLORREF).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Decode the low three bytes of a `COLORREF` (`0x00BBGGRR`).
    pub fn from_colorref(v: i32) -> Self {
        let v = v as u32;
        Self {
            r: (v & 0xff) as u8,
            g: ((v >> 8) & 0xff) as u8,
            b: ((v >> 16) & 0xff) as u8,
        }
    }

    /// Re-encode into `previous`, keeping its top byte.
    ///
    /// REAPER stores flags up there — most visibly, keys it considers unset
    /// come back as large negatives. Rewriting the whole word would silently
    /// flip a key from "unset" to "set to this color", so the color bytes are
    /// the only ones we touch.
    pub fn to_colorref_preserving(self, previous: i32) -> i32 {
        let high = (previous as u32) & 0xff00_0000;
        let low = ((self.b as u32) << 16) | ((self.g as u32) << 8) | self.r as u32;
        (high | low) as i32
    }

    /// Encode as a plain opaque `COLORREF`.
    pub fn to_colorref(self) -> i32 {
        self.to_colorref_preserving(0)
    }

    /// `#rrggbb`.
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Parse `#rgb` / `#rrggbb` (leading `#` optional).
    pub fn parse_hex(s: &str) -> Result<Self> {
        let h = s.trim().trim_start_matches('#');
        let v = |i: usize, n: usize| -> Result<u8> {
            let part = &h[i..i + n];
            let byte = u8::from_str_radix(part, 16)?;
            // #rgb expands each nibble: f -> ff, not f0.
            Ok(if n == 1 { byte * 17 } else { byte })
        };
        match h.len() {
            3 => Ok(Self::new(v(0, 1)?, v(1, 1)?, v(2, 1)?)),
            6 => Ok(Self::new(v(0, 2)?, v(2, 2)?, v(4, 2)?)),
            _ => bail!("expected #rgb or #rrggbb, got {s:?}"),
        }
    }

    fn to_hsl(self) -> Hsl {
        Srgb::new(self.r, self.g, self.b)
            .into_format::<f32>()
            .into_color()
    }

    fn from_hsl(hsl: Hsl) -> Self {
        let rgb: Srgb = Srgb::from_color(hsl);
        let rgb = rgb.into_format::<u8>();
        Self::new(rgb.red, rgb.green, rgb.blue)
    }

    /// Hue in degrees (0–360), saturation and lightness in 0–1.
    pub fn hsl(self) -> (f32, f32, f32) {
        let h = self.to_hsl();
        (
            h.hue.into_positive_degrees(),
            h.saturation.clamp(0.0, 1.0),
            h.lightness.clamp(0.0, 1.0),
        )
    }

    /// Rebuild from hue (degrees), saturation and lightness (0–1).
    pub fn from_hsl_parts(h: f32, s: f32, l: f32) -> Self {
        Self::from_hsl(Hsl::new(h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0)))
    }

    /// Rotate the hue by `degrees`, keeping saturation and lightness.
    pub fn rotate_hue(self, degrees: f32) -> Self {
        let (h, s, l) = self.hsl();
        Self::from_hsl_parts(h + degrees, s, l)
    }

    /// Relative luminance (WCAG), for deciding readable foregrounds.
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
}
