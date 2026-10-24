//! The note-blob document — the Melodyne insight as a data model.
//!
//! Each detected note's pitch contour decomposes into three
//! independently editable terms:
//!
//! ```text
//! f0_midi(t) = center + drift(t)·drift_amount + modulation(t)·modulation_amount
//! ```
//!
//! - **center** — the note's pitch center (median MIDI). Dragging a
//!   blob edits ONLY this; snapping quantizes ONLY this.
//! - **drift** — the slow deviation of the contour from center (the
//!   singer sliding flat over a long note). Zero-phase low-pass of the
//!   residual, ≲ ~3 Hz.
//! - **modulation** — the vibrato residual (contour − center − drift).
//!
//! Melodyne's "pitch drift" and "pitch modulation" tools are exactly
//! the two `*_amount` scalars (1 = as sung, 0 = flattened).
//!
//! Timing edits use the melonix marker model (MIT — mika314/melonix):
//! sample-anchored [`WarpMarker`]s define a piecewise-linear time-warp
//! and a pitch-bend overlay, interpolated between markers and decaying
//! to identity outside them.

use crate::tune::detect::{hz_to_midi, midi_to_hz};
use crate::tune::tracker::TrackedNote;

/// Editable pitch blob for one note.
#[derive(Clone, Debug)]
pub struct NoteBlob {
    /// First analysis frame (inclusive).
    pub start_frame: usize,
    /// Last analysis frame (inclusive).
    pub end_frame: usize,
    /// Pitch center in MIDI (float, cents-accurate). The handle.
    pub center_midi: f64,
    /// The center as analyzed (so edits serialize as deltas and a
    /// re-analysis can preserve them).
    pub analyzed_center_midi: f64,
    /// Slow deviation from center, semitones per frame.
    pub drift: Vec<f64>,
    /// Vibrato residual, semitones per frame.
    pub modulation: Vec<f64>,
    /// 0..1 — scale the sung drift (0 = perfectly flat approach).
    pub drift_amount: f64,
    /// 0..1+ — scale the sung vibrato (0 = robotic, >1 = exaggerated).
    pub modulation_amount: f64,
    /// Per-note formant shift in semitones (0 = as sung).
    pub formant_shift: f64,
    /// Per-note gain trim in dB.
    pub gain_db: f64,
    /// Onset transition: seconds to glide from the previous pitch
    /// into this note's target (0 = hard-tune jump).
    pub retune_s: f64,
    /// Mean frame RMS from analysis (display weighting).
    pub rms: f64,
}

impl NoteBlob {
    /// Frame count.
    #[allow(clippy::len_without_is_empty)] // a blob is never empty
    pub fn len(&self) -> usize {
        self.end_frame - self.start_frame + 1
    }

    /// The edited target contour at `frame` (absolute frame index),
    /// in MIDI. `None` outside the blob.
    pub fn target_midi(&self, frame: usize) -> Option<f64> {
        if frame < self.start_frame || frame > self.end_frame {
            return None;
        }
        let i = frame - self.start_frame;
        Some(
            self.center_midi
                + self.drift[i] * self.drift_amount
                + self.modulation[i] * self.modulation_amount,
        )
    }

    /// Transpose the blob by `semitones` (drag).
    pub fn transpose(&mut self, semitones: f64) {
        self.center_midi += semitones;
    }

    /// The user's pitch edit relative to analysis (for delta storage).
    pub fn center_delta(&self) -> f64 {
        self.center_midi - self.analyzed_center_midi
    }
}

/// Zero-phase one-pole low-pass (forward + backward pass) over a
/// frame-rate signal — extracts the drift term without phase lag so
/// the vibrato residual stays time-aligned.
fn zero_phase_lowpass(x: &[f64], cutoff_hz: f64, frame_rate: f64) -> Vec<f64> {
    if x.is_empty() {
        return Vec::new();
    }
    let coeff = 1.0 - (-core::f64::consts::TAU * cutoff_hz / frame_rate).exp();
    let mut fwd = Vec::with_capacity(x.len());
    let mut state = x[0];
    for &v in x {
        state += (v - state) * coeff;
        fwd.push(state);
    }
    let mut state = *fwd.last().unwrap();
    let mut out = vec![0.0; x.len()];
    for i in (0..x.len()).rev() {
        state += (fwd[i] - state) * coeff;
        out[i] = state;
    }
    out
}

/// Drift cutoff: below this is "sliding", above is "vibrato".
/// Typical vibrato sits at 4–7 Hz; drift well under 2 Hz.
const DRIFT_CUTOFF_HZ: f64 = 3.0;

/// Decompose a tracked note into an editable blob.
///
/// `hop`/`sample_rate` give the analysis frame rate for the
/// drift/vibrato split.
pub fn decompose(note: &TrackedNote, hop: usize, sample_rate: f64) -> NoteBlob {
    let center = note.median_midi;
    let residual: Vec<f64> = note.f0.iter().map(|&hz| hz_to_midi(hz) - center).collect();
    let frame_rate = sample_rate / hop.max(1) as f64;
    let drift = zero_phase_lowpass(&residual, DRIFT_CUTOFF_HZ, frame_rate);
    let modulation: Vec<f64> = residual.iter().zip(&drift).map(|(r, d)| r - d).collect();
    NoteBlob {
        start_frame: note.start_frame,
        end_frame: note.end_frame,
        center_midi: center,
        analyzed_center_midi: center,
        drift,
        modulation,
        drift_amount: 1.0,
        modulation_amount: 1.0,
        formant_shift: 0.0,
        gain_db: 0.0,
        retune_s: 0.0,
        rms: 0.0,
    }
}

/// Melonix-style warp/bend marker (sample-anchored).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WarpMarker {
    /// Anchor position in ORIGINAL samples.
    pub sample: f64,
    /// Time offset in samples applied at the anchor (piecewise-linear
    /// between markers, 0 outside the marker range).
    pub d_time: f64,
    /// Extra pitch bend in semitones at the anchor (same interpolation).
    pub pitch_bend: f64,
}

/// The per-clip pitch document: blobs + warp markers.
#[derive(Clone, Debug, Default)]
pub struct PitchDoc {
    pub blobs: Vec<NoteBlob>,
    /// Sorted by `sample`.
    pub markers: Vec<WarpMarker>,
    /// Analysis hop (samples) — target curves are frame-indexed.
    pub hop: usize,
    pub sample_rate: f64,
}

impl PitchDoc {
    /// Build from tracked notes.
    pub fn from_notes(notes: &[TrackedNote], hop: usize, sample_rate: f64) -> Self {
        Self {
            blobs: notes
                .iter()
                .map(|n| decompose(n, hop, sample_rate))
                .collect(),
            markers: Vec::new(),
            hop,
            sample_rate,
        }
    }

    /// Interpolate the marker pitch-bend curve at an original-sample
    /// position (semitones; 0 outside the marker range).
    pub fn bend_at(&self, sample: f64) -> f64 {
        interp_markers(&self.markers, sample, |m| m.pitch_bend)
    }

    /// Warped time offset at an original-sample position (samples).
    pub fn warp_at(&self, sample: f64) -> f64 {
        interp_markers(&self.markers, sample, |m| m.d_time)
    }

    /// Insert a marker keeping `markers` sorted.
    pub fn add_marker(&mut self, m: WarpMarker) {
        let idx = self.markers.partition_point(|x| x.sample < m.sample);
        self.markers.insert(idx, m);
    }

    /// The edited TARGET pitch contour per analysis frame across the
    /// whole clip: blob targets + marker bend; `None` where no blob is
    /// active (unvoiced — leave untouched). This is what both the
    /// realtime preview shifter and the offline renderer consume.
    pub fn target_curve(&self, n_frames: usize) -> Vec<Option<f64>> {
        let mut out = vec![None; n_frames];
        for blob in &self.blobs {
            let end = blob.end_frame.min(n_frames.saturating_sub(1));
            for (frame, slot) in out
                .iter_mut()
                .enumerate()
                .take(end + 1)
                .skip(blob.start_frame)
            {
                let sample = frame as f64 * self.hop as f64;
                *slot = blob.target_midi(frame).map(|m| m + self.bend_at(sample));
            }
        }
        out
    }

    /// Per-frame shift ratios versus the analyzed contour — feed to a
    /// pitch shifter for preview (`1.0` where inactive/unvoiced).
    pub fn shift_ratios(&self, analyzed_f0_hz: &[Option<f64>]) -> Vec<f64> {
        let targets = self.target_curve(analyzed_f0_hz.len());
        analyzed_f0_hz
            .iter()
            .zip(&targets)
            .map(|(f0, t)| match (f0, t) {
                (Some(hz), Some(target_midi)) if *hz > 0.0 => midi_to_hz(*target_midi) / hz,
                _ => 1.0,
            })
            .collect()
    }
}

fn interp_markers<F: Fn(&WarpMarker) -> f64>(markers: &[WarpMarker], sample: f64, field: F) -> f64 {
    if markers.is_empty() {
        return 0.0;
    }
    if sample <= markers[0].sample {
        return field(&markers[0]);
    }
    if sample >= markers[markers.len() - 1].sample {
        return field(&markers[markers.len() - 1]);
    }
    let idx = markers.partition_point(|m| m.sample <= sample);
    let (a, b) = (&markers[idx - 1], &markers[idx]);
    let span = (b.sample - a.sample).max(1.0e-9);
    let t = (sample - a.sample) / span;
    field(a) + (field(b) - field(a)) * t
}
