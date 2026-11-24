//! The golden session's timeline: tempo and meter changes (4/4, then 7/8,
//! then 3/4, then 4/4 again), a section of every kind and colour, markers
//! coloured and plain, the edit cursor with a time selection, and the play
//! cursor.

use arrangement_view::cursor::{Edit, Span};
use daw_ui::studio::project::{Marker, Section, TempoChange};

use super::tracks::COLOURS;

/// Where the tempo or the meter changes. The project's tempo carries no
/// ramp, so a change is a step.
pub const TEMPO: [TempoChange; 5] = [
    change(0.0, 120.0, 4, 4),
    // Bar 17 at 120 in 4/4.
    change(32.0, 140.0, 7, 8),
    // Eight bars of 7/8 at 140: 3.5 quarter notes each, 1.5 s a bar.
    change(44.0, 100.0, 3, 4),
    // Eight bars of 3/4 at 100: 1.8 s each.
    change(58.4, 90.0, 4, 4),
    change(80.0, 120.0, 4, 4),
];

const fn change(at: f64, bpm: f64, beats_per_bar: u32, beat_unit: u32) -> TempoChange {
    TempoChange {
        at,
        bpm,
        beats_per_bar,
        beat_unit,
    }
}

/// Where the play cursor stands.
pub const PLAY_AT: f64 = 13.5;

/// The song's sections: the usual parts of a song, each its own colour,
/// one with none and one on the ruler's second lane.
#[must_use]
pub fn sections() -> Vec<Section> {
    const PARTS: [(f64, f64, &str, Option<usize>, u32); 10] = [
        (0.0, 8.0, "Intro", Some(0), 0),
        (8.0, 24.0, "Verse 1", Some(2), 0),
        (24.0, 32.0, "Pre-Chorus", Some(4), 0),
        (32.0, 44.0, "Chorus", Some(6), 0),
        (44.0, 58.4, "Verse 2", Some(2), 0),
        (58.4, 70.0, "Bridge", Some(9), 0),
        (70.0, 80.0, "Solo", Some(11), 0),
        (80.0, 92.0, "Outro", Some(13), 0),
        (92.0, 96.0, "Tag", None, 0),
        (32.0, 58.4, "Odd meters", Some(15), 1),
    ];
    PARTS
        .iter()
        .zip(1_u32..)
        .map(|(&(start, end, name, colour, lane), id)| Section {
            id,
            start,
            end,
            name: name.to_owned(),
            color: colour.and_then(|i| COLOURS.get(i)).map(|c| css(*c)),
            lane,
        })
        .collect()
}

/// The markers: numbered flags, coloured and plain, one on the second lane.
#[must_use]
pub fn markers() -> Vec<Marker> {
    const FLAGS: [(f64, &str, Option<usize>, u32); 6] = [
        (4.0, "Count-in ends", None, 0),
        (16.0, "Vocal in", Some(1), 0),
        (36.0, "Hit", Some(3), 0),
        (50.0, "Key change", Some(7), 0),
        (66.0, "Drop", Some(12), 0),
        (88.0, "Fade", None, 1),
    ];
    FLAGS
        .iter()
        .zip(1_u32..)
        .map(|(&(at, name, colour, lane), idx)| Marker {
            at,
            name: name.to_owned(),
            color: colour.and_then(|i| COLOURS.get(i)).map(|c| css(*c)),
            idx,
            lane,
        })
        .collect()
}

/// The edit cursor at 20 s, with a time selection from 24 to 30 s.
#[must_use]
pub const fn cursor() -> Edit {
    Edit {
        at: 20.0,
        selection: Some(Span {
            start: 24.0,
            end: 30.0,
        }),
    }
}

/// `0xRRGGBB` as CSS, the way the project's read-back writes it.
fn css(c: u32) -> String {
    format!("#{:06x}", c & 0x00ff_ffff)
}
