//! The Chart Editor's options: a chart's Keyflow source rewritten by
//! Keyflow's own transposer (`keyflow::transpose_source`), never by one of
//! Studio's.
//!
//! What its toolbar runs (`extension.toml`'s `chart-editor`): transpose a
//! semitone down or up, and write its chords as letters, Nashville numbers
//! or Roman numerals. Each keeps the rest of the source byte for byte
//! (titles, tempo and meter, section headers, rhythm marks, comments).

use keyflow::{ChartView, Key, MusicalNote, NotationSystem, Note as _, transpose_source};

/// A notation the chords can be written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notation {
    Letters,
    Nashville,
    Roman,
}

impl Notation {
    /// By its name in an action's arguments (`letters`, `nashville`,
    /// `roman`).
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "letters" => Some(Self::Letters),
            "nashville" => Some(Self::Nashville),
            "roman" => Some(Self::Roman),
            _ => None,
        }
    }

    const fn system(self) -> NotationSystem {
        match self {
            Self::Letters => NotationSystem::Letters,
            Self::Nashville => NotationSystem::Nashville,
            Self::Roman => NotationSystem::Roman,
        }
    }
}

/// `source` transposed `by` semitones (down when negative): its key and
/// every chord, spelled with sharps going up and flats going down. A chart
/// that names no key is read in C major, as Keyflow reads it.
///
/// Keyflow rewrites the key on the header line only when it leads the line
/// (`#D 100bpm 2/4`); later on it (`100bpm 2/4 #D`) the chords move and the
/// key stays (FastTrackStudios/keyflow#63).
///
/// # Errors
///
/// It does not parse.
pub fn transpose(source: &str, by: i8) -> Result<String, String> {
    let chart = keyflow::parse(source).map_err(|e| e.to_string())?;
    let key = chart
        .initial_key
        .or(chart.current_key)
        .unwrap_or_else(|| Key::major(MusicalNote::from_semitone(0, true)));
    let semitone = i16::from(key.root.semitone())
        .saturating_add(i16::from(by))
        .rem_euclid(12);
    let root = MusicalNote::from_semitone(u8::try_from(semitone).unwrap_or(0), by > 0);
    let view = ChartView {
        target_key: Some(Key::new(root, key.mode)),
        ..ChartView::default()
    };
    Ok(transpose_source(source, &view))
}

/// `source` with its chords written in `notation`, in its own key.
#[must_use]
pub fn renotate(source: &str, notation: Notation) -> String {
    let view = ChartView {
        notation: notation.system(),
        ..ChartView::default()
    };
    transpose_source(source, &view)
}
