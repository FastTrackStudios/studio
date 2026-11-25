//! Fonts for rendering: the host's [`FontBook`], plus the fonts a
//! document carries as `font/*` assets.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::document::proto::Scene;
pub use crate::render::font_book::{
    BUNDLED, BundledFamily, Chain, DEFAULT_FAMILY, Face, FontBook, FontBytes, FontQuery, FontSpec,
    Resolved, StyleHints, parse_style,
};

pub use crate::render::anyrender_renderer::text_font;

/// Whether an asset's mime type is a font.
#[must_use]
pub fn is_font_mime(mime: &str) -> bool {
    let m = mime.to_ascii_lowercase();
    m.starts_with("font/")
        || matches!(
            m.as_str(),
            "application/font-sfnt"
                | "application/x-font-ttf"
                | "application/x-font-otf"
                | "application/vnd.ms-opentype"
        )
}

/// Faces read from document assets, by asset id — read once, so their ids
/// (and every glyph cache keyed on them) stay put frame to frame.
type Faces = Arc<Vec<Arc<Face>>>;

static DOC_FACES: std::sync::LazyLock<Mutex<HashMap<String, Faces>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

fn doc_faces(id: &str, bytes: &[u8]) -> Faces {
    if let Some(hit) = DOC_FACES.lock().ok().and_then(|c| c.get(id).cloned()) {
        return hit;
    }
    let mut book = FontBook::new();
    book.register(bytes.to_vec());
    let faces = Arc::new(book.faces().to_vec());
    if let Ok(mut c) = DOC_FACES.lock() {
        if c.len() > 256 {
            c.clear();
        }
        c.insert(id.to_string(), faces.clone());
    }
    faces
}

/// `base` with the fonts `scene` carries (its `font/*` assets) registered
/// over it — a shared surface brings its own lettering. Borrowed as is
/// when the document has none.
#[must_use]
pub fn book_for_scene<'a>(base: &'a FontBook, scene: &Scene) -> Cow<'a, FontBook> {
    let mut fonts = scene
        .assets
        .values()
        .filter(|a| is_font_mime(&a.mime))
        .peekable();
    if fonts.peek().is_none() {
        return Cow::Borrowed(base);
    }
    let mut book = base.clone();
    for asset in fonts {
        for face in doc_faces(&asset.id.0, &asset.bytes).iter() {
            book.insert_face(face.clone());
        }
    }
    Cow::Owned(book)
}

/// The advance width of `text` set in `font` at `size` px with
/// `letter_spacing` px after each character, measured the way the painter
/// sets it (the resolved face's instance, per-glyph fallback, synthetic
/// bold) — so a host puts a caret exactly where the painted glyphs end.
#[must_use]
pub fn text_advance(
    fonts: &FontBook,
    font: &FontSpec,
    text: &str,
    size: f64,
    letter_spacing: f64,
) -> f64 {
    fonts
        .chain(&font.query())
        .advance(text, size.max(1.0), letter_spacing)
}
