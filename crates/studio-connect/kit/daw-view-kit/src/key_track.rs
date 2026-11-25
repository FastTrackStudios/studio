//! The key track as a view reads it: a `KEY` track whose items name the
//! key from there on (`"Eb major"`), the way a song keeps its key changes
//! where a person can see, move and edit them.

use keyflow::key::Key;
use keyflow::key::scale::ScaleMode;
use keyflow::primitives::MusicalNote;

/// The track key changes live on.
pub const KEY_TRACK: &str = "KEY";

/// Whether `name` is the key track's.
#[must_use]
pub fn is_key_track(name: &str) -> bool {
    name.trim().eq_ignore_ascii_case(KEY_TRACK)
}

/// A key change's label, `"Eb major"` — the inverse of [`format_key`].
/// `None` for a label that names no key.
#[must_use]
pub fn parse_key(label: &str) -> Option<Key> {
    let text = label.trim();
    let (root_text, mode_text) = text.split_once(char::is_whitespace)?;
    let root = MusicalNote::from_string(&normalize_root(root_text.trim()))?;
    let mode = match mode_text.trim().to_ascii_lowercase().as_str() {
        "major" | "ionian" => ScaleMode::ionian(),
        "minor" | "aeolian" => ScaleMode::aeolian(),
        "dorian" => ScaleMode::dorian(),
        "phrygian" => ScaleMode::phrygian(),
        "lydian" => ScaleMode::lydian(),
        "mixolydian" => ScaleMode::mixolydian(),
        "locrian" => ScaleMode::locrian(),
        _ => return None,
    };
    Some(Key::new(root, mode))
}

/// A key as its change is labelled: the common names for Ionian and
/// Aeolian (`"C major"`, not `"C ionian"`), the mode's name otherwise.
#[must_use]
pub fn format_key(key: &Key) -> String {
    let mode = match key.mode.name() {
        "Ionian" => "major".to_owned(),
        "Aeolian" => "minor".to_owned(),
        other => other.to_lowercase(),
    };
    format!("{} {mode}", normalize_root(&key.root.name))
}

/// A note name as the label writes it: its first letter upper case, the
/// rest lower (`eb` → `Eb`).
fn normalize_root(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        let rest: String = chars.collect();
        format!(
            "{}{}",
            first.to_ascii_uppercase(),
            rest.to_ascii_lowercase()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_round_trips() {
        for label in ["C major", "Eb minor", "F# dorian"] {
            let key = parse_key(label).expect(label);
            assert_eq!(format_key(&key), label);
        }
        assert!(parse_key("Intro").is_none());
        assert!(is_key_track(" key "));
    }
}
