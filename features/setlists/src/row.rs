//! What a setlist's row says of its song, wherever it is drawn.
//!
//! The share page's block (`share`) and the setlist view of Studio's web
//! bundle (`studio-island`) both draw a row's tile from these, so the same
//! song has the same key and the same colour on both.

use std::fmt::Write as _;

use crate::keys::{KeyLabel, Played};
use crate::setlist::Line;

/// What a row's tile says of the song's key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowKey {
    /// The key as played (`E`, `E*`, `E→F`).
    pub label: KeyLabel,
    /// Transposed: from the chart's own key, by this many semitones (the
    /// shorter way, −5…+6, never 0).
    pub moved: Option<(String, i32)>,
}

impl RowKey {
    /// Said in words, for a screen reader: "Key of F, transposed up 1
    /// semitone from E".
    #[must_use]
    pub fn spoken(&self) -> String {
        let mut said = self.label.spoken();
        if let Some((from, semitones)) = &self.moved {
            let n = semitones.unsigned_abs();
            let _ = write!(
                said,
                ", transposed {} {n} semitone{} from {}",
                if *semitones < 0 { "down" } else { "up" },
                if n == 1 { "" } else { "s" },
                key_words(from)
            );
        }
        said
    }
}

/// The key a row says ([`Line::key_label`]), in the setlist's key if moved.
///
/// Played in the setlist's key when it names another
/// ([`KeyLabel::played`]); a key it names that is refused (not a key's
/// name, the chart's other mode) is ignored: the row shows the song as
/// charted (`setlist::problems` says why).
#[must_use]
pub fn row_key(line: &Line) -> Option<RowKey> {
    let own = line.key_label()?;
    Some(match line.play.as_deref().map(|p| own.played(p)) {
        Some(Played::Moved { label, semitones }) => RowKey {
            label,
            moved: Some((own.start, semitones)),
        },
        Some(Played::AsCharted | Played::Refused(_)) | None => RowKey {
            label: own,
            moved: None,
        },
    })
}

/// `+4`, `−3` (a minus sign, U+2212).
#[must_use]
pub fn signed(semitones: i32) -> String {
    if semitones < 0 {
        format!("\u{2212}{}", semitones.unsigned_abs())
    } else {
        format!("+{semitones}")
    }
}

/// A key's name in words (`E flat minor`).
fn key_words(key: &str) -> String {
    KeyLabel::plain(key)
        .spoken()
        .trim_start_matches("Key of ")
        .to_owned()
}

/// The hue of a song's tile, from its title (any case): FNV-1a, so the
/// same title is the same colour everywhere (Architect's player draws a
/// tile for a song with no row the same way).
#[must_use]
pub fn tile_hue(title: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for unit in title.to_lowercase().encode_utf16() {
        hash ^= u32::from(unit);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash % 360
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_moved_key_is_said_with_how_far() {
        let line = Line::parse("1 | Silent Night | S.md | E · 60 bpm | | | | E | F").unwrap();
        let key = row_key(&line).unwrap();
        assert_eq!(key.label.text(), "F");
        assert_eq!(key.moved, Some(("E".to_owned(), 1)));
        assert_eq!(key.spoken(), "Key of F, transposed up 1 semitone from E");
        assert_eq!(signed(-3), "\u{2212}3");
    }
}
