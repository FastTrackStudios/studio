//! An arpeggiator bound to a set of chords.
//!
//! The counterpart to [`crate::ops::velocity::Session`], and deliberately a
//! different shape, because the two tools do different things.
//!
//! Velocity editing is *in place*: every note keeps its identity and only
//! its velocity moves, so a session holds a baseline and every control is
//! non-destructive. Arpeggiating is *generative*: the source chord is
//! replaced by a stream of new notes that has no per-note correspondence
//! with what was there. There is nothing to blend back toward.
//!
//! So this session holds the **chords** rather than the notes: the source
//! is parsed once into [`Chord`]s, and the output is recomputed from them
//! on every parameter change. That still gives the property that matters
//! — the controls are parameters, not accumulating actions, and dialling
//! the rate back and forth doesn't compound.

use super::{Arp, ArpNote, Chord, Direction, Step};

/// A live arpeggiator over a fixed set of chords.
#[derive(Clone, Debug, Default)]
pub struct ArpSession {
    /// The source chords, parsed once when the session opened.
    chords: Vec<Chord>,
    /// Indices of the source notes in the take, so the caller knows what
    /// to remove when committing.
    source_indices: Vec<u32>,
    pub arp: Arp,
}

impl ArpSession {
    pub fn new(chords: Vec<Chord>, source_indices: Vec<u32>) -> Self {
        Self {
            chords,
            source_indices,
            arp: Arp::default(),
        }
    }

    pub fn chords(&self) -> &[Chord] {
        &self.chords
    }

    /// Take indices of the notes this session was built from — the ones a
    /// commit replaces.
    pub fn source_indices(&self) -> &[u32] {
        &self.source_indices
    }

    pub fn is_empty(&self) -> bool {
        self.chords.is_empty()
    }

    /// Every chord's notes, for the UI to display what it's working on.
    pub fn chord_labels(&self) -> Vec<String> {
        self.chords
            .iter()
            .map(|c| {
                c.notes
                    .iter()
                    .map(|n| pitch_name(n.pitch))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect()
    }

    // ── Controls ───────────────────────────────────────────────────
    //
    // The arp's step grid is the general form, but a uniform arp — one
    // rate, one gate — is the overwhelmingly common case, so it gets
    // direct accessors rather than making every caller index `steps[0]`.

    pub fn direction(&self) -> Direction {
        self.arp.direction
    }

    pub fn set_direction(&mut self, direction: Direction) {
        self.arp.direction = direction;
    }

    pub fn octaves(&self) -> u8 {
        self.arp.octaves.max(1)
    }

    pub fn set_octaves(&mut self, octaves: u8) {
        self.arp.octaves = octaves.clamp(1, 4);
    }

    pub fn rate_ppq(&self) -> f64 {
        self.arp
            .steps
            .first()
            .map(|s| s.rate_ppq)
            .unwrap_or(super::PPQ / 4.0)
    }

    /// Set the rate on every step, preserving each step's other fields.
    pub fn set_rate_ppq(&mut self, rate_ppq: f64) {
        if self.arp.steps.is_empty() {
            self.arp.steps.push(Step::default());
        }
        for step in &mut self.arp.steps {
            step.rate_ppq = rate_ppq;
        }
    }

    pub fn gate(&self) -> f64 {
        self.arp.steps.first().map(|s| s.gate).unwrap_or(0.9)
    }

    pub fn set_gate(&mut self, gate: f64) {
        if self.arp.steps.is_empty() {
            self.arp.steps.push(Step::default());
        }
        for step in &mut self.arp.steps {
            step.gate = gate.clamp(0.05, 2.0);
        }
    }

    pub fn ratchet(&self) -> u8 {
        self.arp
            .steps
            .first()
            .map(|s| s.ratchet.max(1))
            .unwrap_or(1)
    }

    pub fn set_ratchet(&mut self, ratchet: u8) {
        if self.arp.steps.is_empty() {
            self.arp.steps.push(Step::default());
        }
        for step in &mut self.arp.steps {
            step.ratchet = ratchet.clamp(1, 8);
        }
    }

    /// The notes a commit would write.
    pub fn resolve(&self) -> Vec<ArpNote> {
        self.arp.arpeggiate(&self.chords)
    }
}

/// `60` → `C4`. For labelling chords in the UI.
fn pitch_name(pitch: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    // Middle C = C4 = 60, REAPER's default convention.
    let octave = i16::from(pitch) / 12 - 1;
    format!("{}{}", NAMES[usize::from(pitch % 12)], octave)
}
