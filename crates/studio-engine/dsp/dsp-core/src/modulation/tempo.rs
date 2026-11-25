//! Tempo sync utilities — beat division table and phase calculation.
//!
//! Based on tiagolr's sync system shared across gate12, filtr, time12, reevr.

/// Beat division for tempo-synced modulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SyncDivision {
    /// Free-running at a rate in Hz.
    FreeHz,
    /// Musical division: numerator/denominator in quarter notes.
    Beat(f64),
    /// Triplet division.
    Triplet(f64),
    /// Dotted division.
    Dotted(f64),
}

impl SyncDivision {
    /// Duration in quarter notes.
    pub fn quarter_notes(&self) -> f64 {
        match *self {
            SyncDivision::FreeHz => 0.0,
            SyncDivision::Beat(qn) => qn,
            SyncDivision::Triplet(qn) => qn * 2.0 / 3.0,
            SyncDivision::Dotted(qn) => qn * 1.5,
        }
    }

    /// Whether this is free-running (not tempo-synced).
    pub fn is_free(&self) -> bool {
        matches!(self, SyncDivision::FreeHz)
    }
}

/// A note value from the one table every synced control picks from
/// ([`crate::time::MusicalTime`]), as a cycle length.
impl From<crate::time::MusicalTime> for SyncDivision {
    fn from(t: crate::time::MusicalTime) -> Self {
        SyncDivision::Beat(t.quarter_notes())
    }
}

/// Transport state from the host DAW.
#[derive(Debug, Clone, Copy, Default)]
pub struct TransportInfo {
    /// Current position in quarter notes (beats).
    pub position_qn: f64,
    /// Tempo in BPM.
    pub tempo_bpm: f64,
    /// Whether the transport is playing.
    pub playing: bool,
}

impl TransportInfo {
    /// Beats per sample at the current tempo.
    pub fn beats_per_sample(&self, sample_rate: f64) -> f64 {
        if sample_rate > 0.0 && self.tempo_bpm > 0.0 {
            self.tempo_bpm / (60.0 * sample_rate)
        } else {
            0.0
        }
    }
}

/// Compute the pattern phase (0..1) from the current beat position.
///
/// `sync_qn` is the pattern length in quarter notes, `phase_offset` is 0..1.
#[inline]
pub fn beat_to_phase(beat_pos: f64, sync_qn: f64, phase_offset: f64) -> f64 {
    if sync_qn <= 0.0 {
        return 0.0;
    }
    let raw = beat_pos / sync_qn + phase_offset;
    raw - raw.floor()
}

/// Compute the pattern phase from a free-running rate in Hz.
#[inline]
pub fn rate_to_phase(rate_pos: f64, phase_offset: f64) -> f64 {
    let raw = rate_pos + phase_offset;
    raw - raw.floor()
}

/// Calculate the countdown in samples until the next pattern-sync boundary.
///
/// Used for quantized pattern switching (e.g., switch on next beat).
pub fn pattern_sync_countdown(
    beat_pos: f64,
    sync_qn: f64,
    sample_rate: f64,
    tempo_bpm: f64,
) -> usize {
    if sync_qn <= 0.0 || tempo_bpm <= 0.0 || sample_rate <= 0.0 {
        return 0;
    }
    let samples_per_qn = 60.0 * sample_rate / tempo_bpm;
    let interval = (sync_qn * samples_per_qn) as usize;
    if interval == 0 {
        return 0;
    }
    let current = (beat_pos * samples_per_qn) as usize;
    (interval - current % interval) % interval
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_value_lasts_as_many_quarter_notes_as_it_says() {
        use crate::time::{Flavour, MusicalTime, NoteValue};
        let qn = |v, f| SyncDivision::from(MusicalTime::new(v, f)).quarter_notes();
        assert_eq!(qn(NoteValue::Quarter, Flavour::Straight), 1.0);
        assert_eq!(qn(NoteValue::Whole, Flavour::Straight), 4.0);
        assert_eq!(qn(NoteValue::Eighth, Flavour::Dotted), 0.75);
        assert!((qn(NoteValue::Quarter, Flavour::Triplet) - 2.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn beat_to_phase_wraps() {
        let p = beat_to_phase(5.0, 4.0, 0.0);
        assert!((p - 0.25).abs() < 1e-10);
    }

    #[test]
    fn beat_to_phase_with_offset() {
        let p = beat_to_phase(0.0, 4.0, 0.5);
        assert!((p - 0.5).abs() < 1e-10);
    }

    #[test]
    fn transport_beats_per_sample() {
        let t = TransportInfo {
            position_qn: 0.0,
            tempo_bpm: 120.0,
            playing: true,
        };
        let bps = t.beats_per_sample(48000.0);
        // 120 BPM = 2 beats/sec, so 2/48000 per sample
        assert!((bps - 2.0 / 48000.0).abs() < 1e-15);
    }

    #[test]
    fn triplet_duration() {
        // 1/4t = 1.0 * 2/3 = 0.6667 QN
        let d = SyncDivision::Triplet(1.0).quarter_notes();
        assert!((d - 2.0 / 3.0).abs() < 1e-10);
    }

    #[test]
    fn dotted_duration() {
        // 1/4. = 1.0 * 1.5 = 1.5 QN
        let d = SyncDivision::Dotted(1.0).quarter_notes();
        assert!((d - 1.5).abs() < 1e-10);
    }
}
