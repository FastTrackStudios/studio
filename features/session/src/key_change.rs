//! Changing a song's key: the plan, before anything is touched.
//!
//! Picking a new key moves the song as a whole. Every item on a
//! non-percussion track gets the interval as its take pitch, the KEY
//! track's labels and the chart's chords follow, and the drums, the
//! click, the count and the guide stay where they are. This module is the
//! pure part — the interval, which tracks, the new key's name — so it can
//! be tested without a DAW; [`crate::key`] and the engine's action apply it.

use keyflow::key::Key;

/// A track as the plan needs to see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanTrack {
    pub guid: String,
    pub name: String,
    /// The folder it sits in, if any.
    pub parent: Option<String>,
    pub is_folder: bool,
}

/// What changing the key does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChangePlan {
    /// The interval, in semitones, the nearer way (−5…+6).
    pub semitones: i32,
    /// The tracks whose items are pitched.
    pub pitched: Vec<String>,
    /// The tracks left alone, with why — percussion, click, count, guide,
    /// or a track of the song's own markup (KEY, CHORD, LINES, HITS).
    pub left_alone: Vec<String>,
    /// The key as the KEY track's labels write it (`"Eb major"`).
    pub key_name: String,
}

/// The interval from `from` to `to`, the nearer way: −5…+6 (a tritone goes
/// up).
#[must_use]
pub fn interval(from: &Key, to: &Key) -> i32 {
    let up = (i32::from(to.root.semitone) - i32::from(from.root.semitone)).rem_euclid(12);
    if up > 6 { up - 12 } else { up }
}

/// Folders of the template that hold percussion.
const PERCUSSION_FOLDERS: [&str; 3] = ["drums", "percussion", "perc"];

/// Tracks that are the song's own markup, not sound.
const MARKUP_TRACKS: [&str; 4] = ["key", "chord", "lines", "hits"];

/// Words that mark a track as not to be pitched when no folder says so.
const UNPITCHED_WORDS: [&str; 30] = [
    "drum",
    "drums",
    "kick",
    "snare",
    "tom",
    "toms",
    "hat",
    "hihat",
    "hh",
    "cymbal",
    "cymbals",
    "overhead",
    "overheads",
    "oh",
    "room",
    "rooms",
    "perc",
    "percussion",
    "shaker",
    "tambourine",
    "tamb",
    "clap",
    "claps",
    "click",
    "count",
    "countin",
    "guide",
    "cue",
    "cues",
    "metronome",
];

/// Whether a name reads as percussion, click, count or guide.
#[must_use]
pub fn name_is_unpitched(name: &str) -> bool {
    name.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| UNPITCHED_WORDS.contains(&word))
}

fn in_percussion_folder(tracks: &[PlanTrack], track: &PlanTrack) -> bool {
    let mut at = track.parent.as_deref();
    // Bounded: a malformed parent loop must not hang the plan.
    for _ in 0..tracks.len() {
        let Some(parent) = at.and_then(|g| tracks.iter().find(|t| t.guid == g)) else {
            return false;
        };
        if PERCUSSION_FOLDERS.contains(&parent.name.trim().to_ascii_lowercase().as_str()) {
            return true;
        }
        at = parent.parent.as_deref();
    }
    false
}

/// Plan changing the song from `from` to `to`.
///
/// The template's Drums and Percussion folders decide first (everything
/// inside them is left alone); the name heuristics catch what sits outside
/// one.
#[must_use]
pub fn plan(tracks: &[PlanTrack], from: &Key, to: &Key) -> KeyChangePlan {
    let mut pitched = Vec::new();
    let mut left_alone = Vec::new();
    for track in tracks.iter().filter(|t| !t.is_folder) {
        let unpitched = MARKUP_TRACKS.contains(&track.name.trim().to_ascii_lowercase().as_str())
            || in_percussion_folder(tracks, track)
            || name_is_unpitched(&track.name);
        if unpitched {
            left_alone.push(track.guid.clone());
        } else {
            pitched.push(track.guid.clone());
        }
    }
    KeyChangePlan {
        semitones: interval(from, to),
        pitched,
        left_alone,
        key_name: crate::key::format_key(to),
    }
}

/// `source` (a chart) with its chords and key line moved to `to`.
#[must_use]
pub fn transpose_chart(source: &str, to: &Key) -> String {
    keyflow::transpose_source(
        source,
        &keyflow::ChartView {
            target_key: Some(to.clone()),
            ..keyflow::ChartView::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::key_from_name;

    fn key(root: &str) -> Key {
        key_from_name(root, true).unwrap()
    }

    fn track(guid: &str, name: &str, parent: Option<&str>, is_folder: bool) -> PlanTrack {
        PlanTrack {
            guid: guid.into(),
            name: name.into(),
            parent: parent.map(Into::into),
            is_folder,
        }
    }

    #[test]
    fn a_root_picked_in_the_songs_mode_is_named_in_that_mode() {
        // C major set to A moves −3 and sounds in A major, so it is
        // labelled A major, never A minor.
        let p = plan(&[], &key("C"), &key("A"));
        assert_eq!(p.semitones, -3);
        assert_eq!(p.key_name, "A major");
        let minor = |r| key_from_name(r, false).unwrap();
        let p = plan(&[], &minor("A"), &minor("C"));
        assert_eq!((p.semitones, p.key_name.as_str()), (3, "C minor"));
    }

    #[test]
    fn the_interval_goes_the_nearer_way() {
        assert_eq!(interval(&key("C"), &key("G")), -5);
        assert_eq!(interval(&key("C"), &key("F")), 5);
        assert_eq!(interval(&key("C"), &key("F#")), 6);
        assert_eq!(interval(&key("G"), &key("C")), 5);
        assert_eq!(interval(&key("E"), &key("E")), 0);
        assert_eq!(interval(&key("A"), &key("C")), 3);
        assert_eq!(interval(&key("C"), &key("A")), -3);
    }

    #[test]
    fn percussion_click_count_and_guide_are_left_alone() {
        let tracks = [
            track("d", "Drums", None, true),
            track("k", "Kick In", Some("d"), false),
            track("x", "Mystery", Some("d"), false),
            track("p", "Percussion", None, true),
            track("c", "Conga", Some("p"), false),
            track("g", "Guitar L", None, false),
            track("b", "Bass", None, false),
            track("cl", "Click", None, false),
            track("gd", "Guide Audio", None, false),
            track("ci", "Count In", None, false),
            track("key", "KEY", None, false),
        ];
        let plan = plan(&tracks, &key("C"), &key("D"));
        assert_eq!(plan.semitones, 2);
        assert_eq!(plan.pitched, ["g", "b"]);
        assert_eq!(plan.left_alone, ["k", "x", "c", "cl", "gd", "ci", "key"]);
        assert_eq!(plan.key_name, "D major");
    }

    #[test]
    fn names_match_whole_words() {
        assert!(name_is_unpitched("Snare Top"));
        assert!(name_is_unpitched("OH L"));
        assert!(!name_is_unpitched("Ohio Choir"));
        assert!(!name_is_unpitched("Tomorrow Pad"));
    }

    #[test]
    fn the_chart_follows() {
        let moved = transpose_chart("Song\n#C 4/4 120bpm\n\nVS\nC G Am F\n", &key("D"));
        assert!(moved.contains("#D"), "{moved}");
        assert!(moved.contains("D A Bm G"), "{moved}");
    }
}
