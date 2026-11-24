//! The sweep as a MIDI file, for rendering through a real kit.
//!
//! [`crate::corpus::synth`] answers the flam question with no download and no
//! bleed. This module asks the same question of real drums: the
//! identical grid, written as MIDI, handed to the `drumgizmo` CLI (GPL,
//! a build-time tool, never in this tree) against CrocellKit or DRSKit.
//! What comes back is fifteen or thirteen channels of a real array with
//! real leakage, and the ground truth is still exact because we authored
//! the notes.
//!
//! ## Timing has to be exact, so the division is chosen for it
//!
//! 1000 ticks per quarter note at 500 000 µs per quarter puts a tick at
//! exactly **0.5 ms**, so every spacing in the sweep — 5 through 60 ms —
//! is a whole number of ticks with nothing to round. A conventional 480
//! PPQ would put 5 ms at 4.8 ticks, and the corpus would be measuring
//! its own rounding error at the tight end of the range where the whole
//! question lives.
//!
//! ## What MIDI cannot carry across
//!
//! Velocity. The synthetic renderer scales amplitude linearly, so a
//! grace at 0.25 is a quarter of the accent's amplitude. DrumGizmo
//! instead *selects a sample* by velocity, from however many the kit
//! recorded, so the rendered level follows the kit's own dynamic
//! staircase. The two curves are therefore not directly comparable
//! velocity-for-velocity — the spacing axis is, which is the axis the
//! knee lives on.
//!
//! Rendering must also be made deterministic at the CLI, or the sample
//! selector picks differently on every run; `fetch-corpus.sh` passes
//! `-p close=1.0,diverse=0.0,random=0.0` for exactly that reason.

use std::io;

use midly::num::{u4, u7, u15, u24, u28};
use midly::{
    Format as MidiFormat, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind,
};

use crate::corpus::flam::FlamCase;

/// Ticks per quarter note. See the module docs — this and [`TEMPO_US`]
/// together make a tick exactly half a millisecond.
pub const PPQ: u16 = 1000;
/// Microseconds per quarter note.
pub const TEMPO_US: u32 = 500_000;
/// General MIDI acoustic snare. Both recommended kits map it.
pub const SNARE_NOTE: u8 = 38;
/// Percussion channel, zero-based.
pub const DRUM_CHANNEL: u8 = 9;
/// The accent's MIDI velocity. Ghosts are a fraction of it.
pub const ACCENT_VELOCITY: u8 = 110;
/// How long a note is held. Drum modules trigger on note-on and ignore
/// the length, but a note-on with no note-off is a malformed file that
/// some readers will complain about.
const GATE_TICKS: u32 = 20;

/// Ticks a time in seconds lands on.
pub fn ticks(secs: f64) -> u32 {
    (secs * 1_000_000.0 / TEMPO_US as f64 * PPQ as f64).round() as u32
}

/// The MIDI velocity a grace fraction becomes.
pub fn velocity(fraction: f64) -> u8 {
    ((fraction * ACCENT_VELOCITY as f64).round() as i64).clamp(1, 127) as u8
}

/// Write the sweep as a standard MIDI file.
pub fn write_sweep<W: io::Write>(cases: &[FlamCase], out: W) -> io::Result<()> {
    let mut absolute: Vec<(u32, u8, u8)> = Vec::with_capacity(cases.len() * 4);
    for case in cases {
        absolute.push((ticks(case.accent_secs), SNARE_NOTE, ACCENT_VELOCITY));
        absolute.push((
            ticks(case.grace_secs),
            SNARE_NOTE,
            velocity(case.grace_velocity),
        ));
    }
    // Note-offs as velocity-zero note-ons: one event stream to sort,
    // and every reader treats them identically.
    let offs: Vec<(u32, u8, u8)> = absolute
        .iter()
        .map(|&(t, note, _)| (t + GATE_TICKS, note, 0))
        .collect();
    absolute.extend(offs);
    // Stable sort on tick alone: two strikes on the same tick cannot
    // happen in this grid, but a note-off landing on a later note-on
    // can, and releasing before re-triggering is the order that keeps
    // a voice-limited engine honest.
    absolute.sort_by_key(|&(t, _, vel)| (t, vel));

    let mut track = Vec::with_capacity(absolute.len() + 2);
    track.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(TEMPO_US))),
    });
    let mut previous = 0u32;
    for (tick, note, vel) in absolute {
        track.push(TrackEvent {
            delta: u28::new(tick - previous),
            kind: TrackEventKind::Midi {
                channel: u4::new(DRUM_CHANNEL),
                message: MidiMessage::NoteOn {
                    key: u7::new(note),
                    vel: u7::new(vel),
                },
            },
        });
        previous = tick;
    }
    track.push(TrackEvent {
        delta: u28::new(PPQ as u32),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });

    let mut smf = Smf::new(Header::new(
        MidiFormat::SingleTrack,
        Timing::Metrical(u15::new(PPQ)),
    ));
    smf.tracks.push(track);
    smf.write_std(out)
}
