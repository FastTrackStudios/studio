//! Text advance widths, measured the way the painter lays text out (the
//! font's default instance, advances plus letter spacing) — so a host can
//! put a text field's caret exactly where the painted glyphs end.

use skrifa::MetadataProvider;
use skrifa::instance::Size;
use skrifa::raw::{FileRef, FontRef};

use crate::TextFontRef;

/// The advance width of `text` at `size` px with `letter_spacing` px after
/// each character; `None` when the font can't be read.
#[must_use]
pub fn text_advance(font: TextFontRef<'_>, text: &str, size: f64, letter_spacing: f64) -> Option<f64> {
    let font: FontRef<'_> = match FileRef::new(font.bytes).ok()? {
        FileRef::Font(f) => f,
        FileRef::Collection(c) => c.get(font.index).ok()?,
    };
    let charmap = font.charmap();
    let location = font.axes().location(std::iter::empty::<(&str, f32)>());
    let metrics = font.glyph_metrics(Size::new(size.max(1.0) as f32), &location);
    Some(
        text.chars()
            .filter(|c| *c != '\n')
            .map(|c| f64::from(metrics.advance_width(charmap.map(c).unwrap_or_default()).unwrap_or_default()) + letter_spacing)
            .sum(),
    )
}
