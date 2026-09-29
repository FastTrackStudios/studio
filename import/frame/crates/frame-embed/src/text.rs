//! Text as outlines.
//!
//! A host scene may have no text of its own (a LICE bitmap, a CPU
//! rasteriser without a glyph cache), and a glyph run's font handle does
//! not survive being replayed into a different anyrender version. Paths
//! do: every glyph is drawn as its outline, cached per (glyph, size,
//! weight), so an embed's labels cost a few fills and replay anywhere.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use kurbo::{Affine, BezPath};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::FileRef;
use skrifa::{FontRef, GlyphId, MetadataProvider};

/// frame's UI face (Inter, variable). Hosts may hand in their own.
pub static INTER: &[u8] = include_bytes!("../../../apps/frame/cli/assets/inter/Inter-Variable.ttf");

/// A font and its outline cache.
pub struct Font {
    bytes: Arc<[u8]>,
    index: u32,
    cache: Mutex<HashMap<(u32, u32, u16), Arc<BezPath>>>,
}

impl std::fmt::Debug for Font {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Font")
            .field("bytes", &self.bytes.len())
            .field("index", &self.index)
            .finish()
    }
}

impl Default for Font {
    fn default() -> Self {
        Self::new(INTER.to_vec(), 0)
    }
}

/// One line laid out: each glyph's outline, already placed.
#[derive(Debug, Clone, Default)]
pub struct Line {
    pub path: BezPath,
    pub width: f64,
}

struct Pen(BezPath);

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((f64::from(x), -f64::from(y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((f64::from(x), -f64::from(y)));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to(
            (f64::from(cx0), -f64::from(cy0)),
            (f64::from(x), -f64::from(y)),
        );
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.curve_to(
            (f64::from(cx0), -f64::from(cy0)),
            (f64::from(cx1), -f64::from(cy1)),
            (f64::from(x), -f64::from(y)),
        );
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}

impl Font {
    #[must_use]
    pub fn new(bytes: Vec<u8>, index: u32) -> Self {
        Self {
            bytes: bytes.into(),
            index,
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn font(&self) -> Option<FontRef<'_>> {
        match FileRef::new(&self.bytes).ok()? {
            FileRef::Font(f) => Some(f),
            FileRef::Collection(c) => c.get(self.index).ok(),
        }
    }

    /// Ascent at `size`, px.
    #[must_use]
    pub fn ascent(&self, size: f64) -> f64 {
        self.font().map_or(size * 0.8, |f| {
            let loc = f.axes().location(std::iter::empty::<(&str, f32)>());
            f64::from(f.metrics(Size::new(size as f32), &loc).ascent)
        })
    }

    /// How wide `text` sets at `size` with `spacing` px between letters.
    #[must_use]
    pub fn width(&self, text: &str, size: f64, spacing: f64) -> f64 {
        let Some(font) = self.font() else {
            return text.chars().count() as f64 * size * 0.6;
        };
        let loc = font.axes().location(std::iter::empty::<(&str, f32)>());
        let gm = font.glyph_metrics(Size::new(size as f32), &loc);
        let cmap = font.charmap();
        text.chars()
            .map(|c| {
                f64::from(
                    gm.advance_width(cmap.map(c).unwrap_or_default())
                        .unwrap_or_default(),
                ) + spacing
            })
            .sum()
    }

    /// `text` as one path, its pen starting at the origin on the baseline.
    #[must_use]
    pub fn line(&self, text: &str, size: f64, spacing: f64, weight: f64) -> Line {
        let Some(font) = self.font() else {
            return Line::default();
        };
        let wght = weight.clamp(100.0, 900.0) as f32;
        let loc = font.axes().location([("wght", wght)]);
        let gm = font.glyph_metrics(Size::new(size as f32), &loc);
        let cmap = font.charmap();
        let outlines = font.outline_glyphs();
        let mut path = BezPath::new();
        let mut pen_x = 0.0;
        for ch in text.chars() {
            let gid = cmap.map(ch).unwrap_or_default();
            if let Some(glyph) = self.glyph(&outlines, gid, size, wght, LocationRef::from(&loc)) {
                path.extend(
                    (Affine::translate((pen_x, 0.0)) * (*glyph).clone())
                        .elements()
                        .iter()
                        .copied(),
                );
            }
            pen_x += f64::from(gm.advance_width(gid).unwrap_or_default()) + spacing;
        }
        Line { path, width: pen_x }
    }

    fn glyph(
        &self,
        outlines: &skrifa::outline::OutlineGlyphCollection<'_>,
        gid: GlyphId,
        size: f64,
        wght: f32,
        loc: LocationRef<'_>,
    ) -> Option<Arc<BezPath>> {
        let key = (gid.to_u32(), (size * 64.0).round() as u32, wght as u16);
        if let Some(hit) = self.cache.lock().ok()?.get(&key) {
            return Some(hit.clone());
        }
        let glyph = outlines.get(gid)?;
        let mut pen = Pen(BezPath::new());
        glyph
            .draw(
                DrawSettings::unhinted(Size::new(size as f32), loc),
                &mut pen,
            )
            .ok()?;
        let path = Arc::new(pen.0);
        if let Ok(mut cache) = self.cache.lock() {
            if cache.len() > 4096 {
                cache.clear();
            }
            cache.insert(key, path.clone());
        }
        Some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_have_outlines_and_widths() {
        let font = Font::default();
        let line = font.line("EQ 4 · 5.5dB", 7.0, 0.0, 400.0);
        assert!(!line.path.elements().is_empty());
        assert!((line.width - font.width("EQ 4 · 5.5dB", 7.0, 0.0)).abs() < 1e-6);
        assert!(font.width("COMP", 7.0, 0.0) > font.width("EQ", 7.0, 0.0));
        assert!(font.ascent(10.0) > 6.0);
    }
}
