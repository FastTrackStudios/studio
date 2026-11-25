//! The Chart Editor's options: Keyflow's transposer on a chart's source.

use studio_views::chart::{Notation, renotate, transpose};

/// The key first on its header line: Keyflow rewrites the key there (one
/// later on the line it leaves as it is, FastTrackStudios/keyflow#63).
const SONG: &str =
    "Joy to the World - Isaac Watts\n#D 100bpm 2/4\n\\downbeat = 1\n\nVS 8\nD D A D\nD G A D\n";

#[test]
fn up_a_tone_moves_the_key_and_every_chord_and_nothing_else() {
    let up = transpose(SONG, 2).unwrap();
    assert!(up.contains("#E"), "{up}");
    assert!(up.contains("E E B E\nE A B E"), "{up}");
    // The rest byte for byte.
    assert!(up.starts_with("Joy to the World - Isaac Watts\n"));
    assert!(up.contains("\\downbeat = 1\n\nVS 8\n"));
}

#[test]
fn down_and_up_again_is_where_it_started() {
    let down = transpose(SONG, -1).unwrap();
    assert!(down.contains("#Db"), "{down}");
    assert_eq!(transpose(&down, 1).unwrap(), SONG);
}

#[test]
fn chords_as_nashville_numbers() {
    let numbers = renotate(SONG, Notation::Nashville);
    assert!(numbers.contains("1 1 5 1\n1 4 5 1"), "{numbers}");
    assert_eq!(Notation::parse("Roman"), Some(Notation::Roman));
    assert_eq!(Notation::parse("solfege"), None);
}
