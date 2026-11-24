//! Text for the recorded scene.
//!
//! Vello draws GLYPHS, not strings: it wants glyph ids with positions,
//! which means the mapping from characters to ids and the advance
//! between them has to happen here. `skrifa` provides both from the font
//! file — the same font the DOM track panel asks for by name.
//!
//! # What this deliberately is not
//!
//! It is not a text shaper. There is no bidi, no script itemisation, no
//! ligature or kerning support, and a character the font has no glyph
//! for is dropped rather than substituted from elsewhere. Track names,
//! record inputs and index numbers are short, left-to-right, and drawn
//! into a fixed-width field, which is exactly the case a shaper is not
//! needed for.
//!
//! When something here needs real shaping — a user's track named in
//! Arabic, say — the answer is parley, not more code in this file. It is
//! written so that swap is a change of one type.

use eyre::{Result, eyre};
use skrifa::instance::Size;
use skrifa::metrics::GlyphMetrics;
use skrifa::{FontRef, MetadataProvider};
use vello::peniko::{Blob, FontData};

/// A loaded font, with the two tables laying text out needs.
pub struct Font {
    data: FontData,
    /// Retained so glyph lookup does not re-parse the file per string.
    inner: FontRef<'static>,
    /// The ASCII characters' glyphs and advances, by size, as laid out
    /// once: a frame of the arrangement measures and lays out thousands
    /// of names, and reading the font's metrics tables for each was a
    /// tenth of the window's time.
    ascii: std::sync::Mutex<Vec<std::sync::Arc<Ascii>>>,
}

/// The 128 ASCII characters at one size: each one's glyph (none when the
/// face has none) and advance.
struct Ascii {
    size: u32,
    glyphs: [Option<(u32, f32)>; 128],
}

/// One positioned glyph, in the same shape `anyrender` records.
pub type Glyph = anyrender::Glyph;

impl Font {
    /// Load the embedded sans face.
    ///
    /// The bytes are compiled in, so a window cannot come up with a
    /// track panel that has no text in it because a file was missing at
    /// runtime — which is what a fontconfig lookup allows on a box that
    /// is not this one.
    ///
    /// # Errors
    ///
    /// If the embedded face does not parse. That cannot depend on the
    /// machine or the run — it would mean the build shipped a corrupt
    /// asset — but it is returned rather than panicked because the
    /// caller is opening a window and can say so.
    pub fn embedded() -> Result<Self> {
        /// `DejaVu Sans`: the fallback the track panel's CSS already
        /// names, so the canvas and the DOM ask for the same face.
        static BYTES: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");

        let inner = FontRef::new(BYTES).map_err(|e| eyre!("the embedded font is corrupt: {e}"))?;
        Ok(Self {
            data: FontData::new(Blob::new(std::sync::Arc::new(BYTES)), 0),
            inner,
            ascii: std::sync::Mutex::default(),
        })
    }

    #[must_use]
    pub const fn data(&self) -> &FontData {
        &self.data
    }

    /// Lay `text` out at `size`, starting from the origin.
    ///
    /// Positions are relative to the text's baseline-left, so the caller
    /// places the run with a transform rather than doing arithmetic on
    /// every glyph.
    #[must_use]
    pub fn layout(&self, text: &str, size: f32) -> Vec<Glyph> {
        if let Some(ascii) = self.ascii_at(text, size) {
            let mut x = 0.0_f32;
            return text
                .bytes()
                .filter_map(|b| ascii.glyphs[usize::from(b)])
                .map(|(id, advance)| {
                    let glyph = Glyph { id, x, y: 0.0 };
                    x += advance;
                    glyph
                })
                .collect();
        }
        let charmap = self.inner.charmap();
        let metrics = GlyphMetrics::new(
            &self.inner,
            Size::new(size),
            skrifa::instance::LocationRef::default(),
        );
        let mut x = 0.0_f32;
        let mut glyphs = Vec::with_capacity(text.len());
        for ch in text.chars() {
            // A character the face cannot draw is skipped rather than
            // rendered as .notdef: a row of empty boxes reads as a
            // rendering bug, where a missing glyph reads as a missing
            // glyph.
            let Some(id) = charmap.map(ch) else { continue };
            glyphs.push(Glyph {
                id: id.to_u32(),
                x,
                y: 0.0,
            });
            x += metrics.advance_width(id).unwrap_or_default();
        }
        glyphs
    }

    /// How wide `text` would be at `size`, without building the run.
    ///
    /// Used to centre and to right-align, and to decide when a name has
    /// to be cut short.
    #[must_use]
    pub fn width(&self, text: &str, size: f32) -> f64 {
        if let Some(ascii) = self.ascii_at(text, size) {
            return f64::from(
                text.bytes()
                    .filter_map(|b| ascii.glyphs[usize::from(b)])
                    .map(|(_, advance)| advance)
                    .sum::<f32>(),
            );
        }
        let charmap = self.inner.charmap();
        let metrics = GlyphMetrics::new(
            &self.inner,
            Size::new(size),
            skrifa::instance::LocationRef::default(),
        );
        f64::from(
            text.chars()
                .filter_map(|ch| charmap.map(ch))
                .filter_map(|id| metrics.advance_width(id))
                .sum::<f32>(),
        )
    }

    /// The longest prefix of `text`, cut at a character, no wider than
    /// `max` at `size` — measured once along it, not again for each cut.
    #[must_use]
    pub fn prefix_within<'t>(&self, text: &'t str, size: f32, max: f64) -> &'t str {
        if let Some(ascii) = self.ascii_at(text, size) {
            let mut used = 0.0_f64;
            let end = text
                .bytes()
                .take_while(|&b| {
                    let advance = ascii.glyphs[usize::from(b)].map_or(0.0, |(_, a)| a);
                    used += f64::from(advance);
                    used <= max
                })
                .count();
            return text.get(..end).unwrap_or("");
        }
        let mut used = 0.0_f64;
        let mut end = 0;
        for (at, ch) in text.char_indices() {
            used += self.width(ch.encode_utf8(&mut [0_u8; 4]), size);
            if used > max {
                break;
            }
            end = at + ch.len_utf8();
        }
        text.get(..end).unwrap_or("")
    }

    /// The ASCII table at `size`, when `text` is all ASCII: built the
    /// first time a size is asked for.
    fn ascii_at(&self, text: &str, size: f32) -> Option<std::sync::Arc<Ascii>> {
        if !text.is_ascii() {
            return None;
        }
        let bits = size.to_bits();
        let mut held = self.ascii.lock().ok()?;
        if let Some(ascii) = held.iter().find(|a| a.size == bits) {
            return Some(std::sync::Arc::clone(ascii));
        }
        let charmap = self.inner.charmap();
        let metrics = GlyphMetrics::new(
            &self.inner,
            Size::new(size),
            skrifa::instance::LocationRef::default(),
        );
        let mut glyphs = [None; 128];
        for (b, slot) in (0_u8..128).zip(glyphs.iter_mut()) {
            *slot = charmap
                .map(char::from(b))
                .map(|id| (id.to_u32(), metrics.advance_width(id).unwrap_or_default()));
        }
        let ascii = std::sync::Arc::new(Ascii { size: bits, glyphs });
        held.push(std::sync::Arc::clone(&ascii));
        Some(ascii)
    }

    /// The longest prefix of `text` that fits `max` wide.
    ///
    /// REAPER truncates rather than scrolling or shrinking, so a long
    /// name simply stops. An ellipsis is added when anything was cut,
    /// which is the only way to tell "Strings" from "Strings (Section
    /// A)" at this width.
    #[must_use]
    pub fn elide(&self, text: &str, size: f32, max: f64) -> String {
        if self.width(text, size) <= max {
            return text.to_owned();
        }
        let ellipsis = self.width("…", size);
        let mut out = String::new();
        let mut used = 0.0;
        for ch in text.chars() {
            let w = self.width(ch.encode_utf8(&mut [0_u8; 4]), size);
            if used + w + ellipsis > max {
                break;
            }
            out.push(ch);
            used += w;
        }
        out.push('…');
        out
    }

    /// The largest size at or below `size` that fits `text` in `max`,
    /// and the text to draw at it.
    ///
    /// Shrink before you cut. A name only has to survive being read —
    /// `Sub`, `Verb`, `T2` are three, four and two characters, and on a
    /// thirty-pixel strip they were coming out as `S…`, `V…`, `T…`
    /// while there was room for all of them a point or two smaller. An
    /// ellipsis that hides more than it reveals is worse than small
    /// type: two of those strips side by side both said `T…`, which is
    /// no name at all.
    ///
    /// REAPER truncates rather than shrinking, and it can afford to —
    /// every strip there is 86 wide. Ours go down to 30, which is the
    /// width at which the choice starts to matter.
    ///
    /// Below `floor` it gives up and elides, because type too small to
    /// read is not a name either.
    #[must_use]
    pub fn fit(&self, text: &str, size: f32, floor: f32, max: f64) -> (String, f32) {
        if max <= 0.0 {
            return (String::new(), size);
        }
        // Half a point at a time: finer than the eye resolves at these
        // sizes, and coarse enough that this is a handful of passes.
        let mut trial = size;
        while trial >= floor {
            if self.width(text, trial) <= max {
                return (text.to_owned(), trial);
            }
            trial -= 0.5;
        }
        (self.elide(text, floor, max), floor)
    }
}
