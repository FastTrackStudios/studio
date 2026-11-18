//! What Studio's painted runtime views share.
//!
//! - [`Font`]: text as glyph runs. A runtime view's scene crosses to the
//!   host as commands, a font's bytes once by its hash, so text is glyph
//!   ids and positions laid out here: no shaper (no bidi, ligatures or
//!   fallback), which track and clip names do not need.
//! - [`Palette`]: the app's theme tokens (`background`, `surface`, `ink`,
//!   `border`, `accent`) as colours, light or dark.
//! - [`rect`], [`text`]: one fill, one glyph run.

use std::sync::{Arc, LazyLock, Mutex};

use anyrender::{Glyph, PaintScene};
use kurbo::{Affine, Rect, Vec2};
use peniko::{Blob, Color, Fill, FontData};
use skrifa::instance::{LocationRef, Size};
use skrifa::metrics::GlyphMetrics;
use skrifa::{FontRef, MetadataProvider};

static BYTES: &[u8] = include_bytes!("../../assets/DejaVuSans.ttf");

/// The embedded face (DejaVu Sans, Session's track panel face).
pub struct Font {
    data: FontData,
    ascii: Mutex<Vec<(u32, Arc<[Option<(u32, f32)>; 128]>)>>,
}

/// The one face, loaded once.
pub static FONT: LazyLock<Font> = LazyLock::new(|| Font {
    data: FontData::new(Blob::new(Arc::new(BYTES)), 0),
    ascii: Mutex::default(),
});

impl Font {
    fn face() -> Option<FontRef<'static>> {
        FontRef::new(BYTES).ok()
    }

    #[must_use]
    pub const fn data(&self) -> &FontData {
        &self.data
    }

    fn ascii(&self, size: f32) -> Option<Arc<[Option<(u32, f32)>; 128]>> {
        let bits = size.to_bits();
        let mut held = self.ascii.lock().ok()?;
        if let Some((_, t)) = held.iter().find(|(s, _)| *s == bits) {
            return Some(Arc::clone(t));
        }
        let face = Self::face()?;
        let charmap = face.charmap();
        let metrics = GlyphMetrics::new(&face, Size::new(size), LocationRef::default());
        let mut table = [None; 128];
        for (b, slot) in (0_u8..128).zip(table.iter_mut()) {
            *slot = charmap
                .map(char::from(b))
                .map(|id| (id.to_u32(), metrics.advance_width(id).unwrap_or_default()));
        }
        let table = Arc::new(table);
        held.push((bits, Arc::clone(&table)));
        Some(table)
    }

    /// `text` at `size` from the origin (baseline left), and its width.
    #[must_use]
    pub fn layout(&self, text: &str, size: f32) -> (Vec<Glyph>, f32) {
        let mut x = 0.0_f32;
        let mut out = Vec::with_capacity(text.len());
        if text.is_ascii()
            && let Some(t) = self.ascii(size)
        {
            for b in text.bytes() {
                if let Some(Some((id, adv))) = t.get(usize::from(b)) {
                    out.push(Glyph { id: *id, x, y: 0.0 });
                    x += adv;
                }
            }
            return (out, x);
        }
        let Some(face) = Self::face() else {
            return (out, 0.0);
        };
        let charmap = face.charmap();
        let metrics = GlyphMetrics::new(&face, Size::new(size), LocationRef::default());
        for ch in text.chars() {
            let Some(id) = charmap.map(ch) else { continue };
            out.push(Glyph {
                id: id.to_u32(),
                x,
                y: 0.0,
            });
            x += metrics.advance_width(id).unwrap_or_default();
        }
        (out, x)
    }

    /// `text` cut to fit `max` wide at `size`, with an ellipsis if cut.
    #[must_use]
    pub fn fit(&self, text: &str, size: f32, max: f32) -> String {
        let (_, w) = self.layout(text, size);
        if w <= max {
            return text.to_owned();
        }
        let (_, dots) = self.layout("…", size);
        let mut out = String::new();
        for ch in text.chars() {
            let mut next = out.clone();
            next.push(ch);
            if self.layout(&next, size).1 + dots > max {
                break;
            }
            out = next;
        }
        if out.is_empty() {
            return String::new();
        }
        out.push('…');
        out
    }
}

/// One filled rectangle.
pub fn rect(scene: &mut impl PaintScene, r: Rect, color: Color) {
    scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &r);
}

/// `text` at `size`, its baseline-left at `(x, y)`, cut to `max` wide.
pub fn text(
    scene: &mut impl PaintScene,
    s: &str,
    size: f32,
    (x, y): (f64, f64),
    max: f32,
    color: Color,
) {
    if max <= 4.0 || s.is_empty() {
        return;
    }
    let fitted = FONT.fit(s, size, max);
    if fitted.is_empty() {
        return;
    }
    let (glyphs, _) = FONT.layout(&fitted, size);
    scene.draw_glyphs(
        FONT.data(),
        size,
        false,
        &[],
        Vec2::ZERO,
        Fill::NonZero,
        color,
        1.0,
        Affine::translate((x, y)),
        None,
        glyphs.into_iter(),
    );
}

/// The colours a view draws with, from the app's theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub dark: bool,
    pub background: Color,
    pub surface: Color,
    pub raised: Color,
    pub ink: Color,
    pub ink_dim: Color,
    pub border: Color,
    pub accent: Color,
}

/// `#rgb`, `#rrggbb` or `#rrggbbaa`.
#[must_use]
pub fn parse(css: &str) -> Option<Color> {
    let h = css.trim().strip_prefix('#')?;
    let n = |s: &str| u8::from_str_radix(s, 16).ok();
    match h.len() {
        3 => {
            let d = |i: usize| n(&h[i..=i]).map(|v| v * 17);
            Some(Color::from_rgb8(d(0)?, d(1)?, d(2)?))
        }
        6 => Some(Color::from_rgb8(n(&h[0..2])?, n(&h[2..4])?, n(&h[4..6])?)),
        8 => Some(Color::from_rgba8(
            n(&h[0..2])?,
            n(&h[2..4])?,
            n(&h[4..6])?,
            n(&h[6..8])?,
        )),
        _ => None,
    }
}

fn luma(c: Color) -> f32 {
    let [r, g, b, _] = c.components;
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// `a` toward `b` by `t`.
#[must_use]
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let (x, y) = (a.components, b.components);
    Color::new([
        x[0] + (y[0] - x[0]) * t,
        x[1] + (y[1] - x[1]) * t,
        x[2] + (y[2] - x[2]) * t,
        x[3] + (y[3] - x[3]) * t,
    ])
}

/// A `0xRRGGBB`.
#[must_use]
pub fn rgb(c: u32) -> Color {
    let [_, r, g, b] = c.to_be_bytes();
    Color::from_rgb8(r, g, b)
}

impl Palette {
    pub const DARK: Self = Self {
        dark: true,
        background: Color::from_rgb8(0x16, 0x17, 0x1a),
        surface: Color::from_rgb8(0x1e, 0x20, 0x24),
        raised: Color::from_rgb8(0x2a, 0x2d, 0x33),
        ink: Color::from_rgb8(0xe6, 0xe8, 0xec),
        ink_dim: Color::from_rgb8(0x8a, 0x8f, 0x98),
        border: Color::from_rgb8(0x33, 0x36, 0x3d),
        accent: Color::from_rgb8(0x4f, 0x7c, 0xff),
    };
    pub const LIGHT: Self = Self {
        dark: false,
        background: Color::from_rgb8(0xf6, 0xf6, 0xf4),
        surface: Color::from_rgb8(0xff, 0xff, 0xff),
        raised: Color::from_rgb8(0xe9, 0xea, 0xec),
        ink: Color::from_rgb8(0x1d, 0x1f, 0x23),
        ink_dim: Color::from_rgb8(0x6b, 0x70, 0x78),
        border: Color::from_rgb8(0xd6, 0xd8, 0xdc),
        accent: Color::from_rgb8(0x2f, 0x5f, 0xe8),
    };

    /// The theme's, with the built-in palette of its lightness for what
    /// it doesn't say (or says in a form this doesn't read).
    #[must_use]
    pub fn of(theme: &architect_workspace::ThemeTokens) -> Self {
        let get = |k: &str| theme.get(k).and_then(parse);
        let background = get("background");
        let dark = background.is_none_or(|b| luma(b) < 0.5);
        let base = if dark { Self::DARK } else { Self::LIGHT };
        let background = background.unwrap_or(base.background);
        let surface = get("surface").unwrap_or(base.surface);
        let ink = get("ink").unwrap_or(base.ink);
        Self {
            dark,
            background,
            surface,
            raised: mix(surface, ink, 0.08),
            ink,
            ink_dim: mix(ink, background, 0.45),
            border: get("border").unwrap_or(base.border),
            accent: get("accent").unwrap_or(base.accent),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_lays_out_and_fits() {
        let (g, w) = FONT.layout("Drums", 12.0);
        assert_eq!(g.len(), 5);
        assert!(w > 20.0 && w < 60.0, "{w}");
        let cut = FONT.fit("A very long track name indeed", 12.0, 60.0);
        assert!(cut.ends_with('…') && FONT.layout(&cut, 12.0).1 <= 60.0);
    }

    #[test]
    fn a_theme_reads_as_a_palette() {
        let light = architect_workspace::ThemeTokens::default()
            .with("background", "#ffffff")
            .with("accent", "#ff0000");
        let p = Palette::of(&light);
        assert!(!p.dark);
        assert_eq!(p.accent, Color::from_rgb8(255, 0, 0));
        assert!(Palette::of(&architect_workspace::ThemeTokens::default()).dark);
        assert_eq!(parse("#abc"), Some(Color::from_rgb8(0xaa, 0xbb, 0xcc)));
    }
}
