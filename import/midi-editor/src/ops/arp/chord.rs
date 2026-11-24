//! Folding a run of notes back into chords.
//!
//! A MIDI take is a flat list of notes; an arpeggiator needs to know
//! which of them were struck together. That's a grouping problem, and
//! getting it wrong is the difference between arpeggiating a progression
//! and arpeggiating one enormous cluster.

use crate::ops::velocity::{MAX_VELOCITY, MIN_VELOCITY};

/// A note as the grouper sees it — position and pitch, no take index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimedNote {
    pub start_ppq: f64,
    pub length_ppq: f64,
    pub pitch: u8,
    pub velocity: u8,
}

impl TimedNote {
    pub fn end_ppq(&self) -> f64 {
        self.start_ppq + self.length_ppq
    }
}

/// One note of a chord.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChordNote {
    pub pitch: u8,
    pub velocity: u8,
}

/// Notes struck together, and the span they occupy.
///
/// `notes` is always sorted low to high. Sorting here rather than per
/// direction is what lets [`super::Cursor`] be a pure index sequence:
/// "up" is ascending index, and nothing downstream has to re-sort.
#[derive(Clone, Debug, PartialEq)]
pub struct Chord {
    pub start_ppq: f64,
    pub end_ppq: f64,
    pub notes: Vec<ChordNote>,
}

impl Chord {
    pub fn length_ppq(&self) -> f64 {
        self.end_ppq - self.start_ppq
    }
}

/// Default slop for "struck together", in PPQ.
///
/// A 32nd note at 960 PPQ. Generous on purpose: a played chord is never
/// perfectly simultaneous, and the alternative — treating a hand-rolled
/// chord as four separate one-note "chords" — turns an arpeggio into four
/// stuttering repeats. Upstream uses 10 ticks, which is about a
/// millisecond and only groups quantized input.
pub const DEFAULT_GAP_PPQ: f64 = 120.0;

/// Group `notes` into chords by **onset**.
///
/// A note joins the current chord if it starts within `gap_ppq` of that
/// chord's *first* note; otherwise it starts a new one. Input need not be
/// sorted.
///
/// ## Why onsets and not "is the chord still sounding"
///
/// Grouping by whether the previous chord has ended is the tempting rule,
/// and it's what upstream does (`startpos - chord[#chord].endpos > 10`).
/// It's wrong for the most ordinary input there is: a block-chord
/// progression, where each chord is held right up to the next one. Am
/// ending exactly where F begins reads as a single eight-note cluster,
/// and the arpeggio comes out as one run over both chords instead of two.
///
/// The cost is that a note struck *later* over a still-held chord — a
/// melody note over a sustained bass — becomes its own chord rather than
/// joining. That's the right trade: it's rarer, it's genuinely ambiguous,
/// and being predictable matters more here than being clever. Widen
/// `gap_ppq` if you want a rolled chord to group anyway.
pub fn group_chords(notes: &[TimedNote], gap_ppq: f64) -> Vec<Chord> {
    let mut sorted: Vec<TimedNote> = notes.to_vec();
    sorted.sort_by(|a, b| {
        a.start_ppq
            .partial_cmp(&b.start_ppq)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.pitch.cmp(&b.pitch))
    });

    let mut chords: Vec<Chord> = Vec::new();
    for note in sorted {
        match chords.last_mut() {
            // Against the chord's own onset, so how *long* the chord is
            // held has no bearing on what belongs to it. See the fn docs.
            Some(chord) if note.start_ppq <= chord.start_ppq + gap_ppq => {
                chord.end_ppq = chord.end_ppq.max(note.end_ppq());
                chord.notes.push(ChordNote {
                    pitch: note.pitch,
                    velocity: note.velocity.clamp(MIN_VELOCITY, MAX_VELOCITY),
                });
            }
            _ => chords.push(Chord {
                start_ppq: note.start_ppq,
                end_ppq: note.end_ppq(),
                notes: vec![ChordNote {
                    pitch: note.pitch,
                    velocity: note.velocity.clamp(MIN_VELOCITY, MAX_VELOCITY),
                }],
            }),
        }
    }

    for chord in &mut chords {
        chord.notes.sort_by_key(|n| n.pitch);
        chord.notes.dedup_by_key(|n| n.pitch);
    }
    chords
}
