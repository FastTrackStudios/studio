//! Cut / copy / paste.
//!
//! Notes are captured **relative to the earliest one**, not at their
//! absolute position, so a paste lands where the pointer is and the
//! internal spacing of a phrase survives the trip. Curves, zone splits,
//! articulations and lyrics all ride along — a copied note is the whole
//! note, not a rectangle.

use crate::doc::{ExpressionDoc, Note, NoteId};

/// Notes lifted out of a document, normalized to their own origin: the
/// shared [`timeline::clipboard::Clipboard`] over notes.
///
/// Deliberately not tied to the system clipboard: this surface runs as
/// a plugin editor inside hosts that route keyboard and clipboard
/// unpredictably, and a paste that silently picks up a spreadsheet cell
/// is worse than one that only ever sees notes. The host adapter can
/// bridge to the OS clipboard where it makes sense.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clipboard(timeline::clipboard::Clipboard<Note>);

/// A note is a clip: its span, its row, moved whole (curves, splits,
/// articulations, lyrics ride along).
impl timeline::clipboard::Clip for Note {
    fn start(&self) -> f64 {
        self.start
    }
    fn end(&self) -> f64 {
        self.end
    }
    fn row(&self) -> i32 {
        self.row
    }
    fn shifted(&self, dt: f64, drows: i32) -> Self {
        let mut n = self.clone();
        n.shift_time(dt);
        n.row = (n.row + drows).clamp(0, 127);
        n
    }
}

impl Clipboard {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// The row the phrase was copied from.
    pub fn origin_row(&self) -> i32 {
        self.0.origin_row()
    }

    /// The phrase's own duration — how much room a paste needs.
    pub fn span(&self) -> f64 {
        self.0.span()
    }

    /// Capture `ids` from `doc`. Ids that no longer exist are skipped;
    /// capturing nothing leaves the previous contents alone, so a
    /// mis-aimed copy does not destroy what you had.
    pub fn copy_from(&mut self, doc: &ExpressionDoc, ids: &[NoteId]) -> bool {
        self.0
            .copy(ids.iter().filter_map(|id| doc.note(*id).cloned()))
    }

    /// The notes a paste at `(t, row)` would produce, with placeholder
    /// ids — [`crate::edit::Edit::PasteNotes`] mints the real ones.
    pub fn placed(&self, t: f64, row: i32) -> Vec<Note> {
        self.0.placed(t, row)
    }
}
