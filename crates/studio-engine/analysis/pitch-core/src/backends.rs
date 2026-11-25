//! Swappable pitch detection, note segmentation, and resynthesis traits.
//!
//! Built-in classical DSP implementations (`fts.pitch.yin`,
//! `fts.segment.contour`, `fts.synth.td-ola`) prove the full ARA-style pipeline
//! with zero ML dependencies, while the exact same traits and JSON document
//! schema accept future neural ONNX backends (FCPE, RMVPE, GAME,
//! PC-NSF-HiFiGAN) or third-party providers.

use serde::{Deserialize, Serialize};

use crate::detect::{PitchFrame, YinConfig, YinDetector, hz_to_midi, midi_to_hz};
use crate::note::{NoteConfig, segment_notes};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendInfo {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FrameRecord {
    pub time_s: f64,
    pub f0_hz: Option<f64>,
    pub aperiodicity: f64,
    pub rms: f64,
}

/// Editable note blob (mirrors Melodyne / PitchNet piano-roll note properties).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EditableNote {
    pub id: String,
    pub start_frame: usize,
    pub end_frame: usize,
    pub start_s: f64,
    pub end_s: f64,
    /// Original detected median pitch (fractional MIDI note).
    pub detected_midi: f64,
    /// User/agent pitch transposition in semitones.
    pub pitch_delta_semitones: f64,
    /// 0.0 = flat pitch center, 1.0 = original pitch drift/vibrato preserved.
    pub pitch_modulation_scale: f64,
    /// Per-note gain trim in dB.
    pub gain_db: f64,
    /// True for breath/sibilant/unvoiced regions that must bypass pitch shifting.
    pub unvoiced_preserve: bool,
}

impl EditableNote {
    pub fn target_midi(&self) -> f64 {
        self.detected_midi + self.pitch_delta_semitones
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PitchAnalysisDoc {
    pub detector_id: String,
    pub segmenter_id: String,
    pub sample_rate: f64,
    pub hop_samples: usize,
    pub total_samples: usize,
    pub frames: Vec<FrameRecord>,
    pub notes: Vec<EditableNote>,
}

impl PitchAnalysisDoc {
    /// Compute per-frame pitch shift ratio (`target_hz / analyzed_hz`), returning
    /// `1.0` on unvoiced/breath/sibilant frames so consonants stay untouched.
    pub fn frame_shift_ratios(&self) -> Vec<f64> {
        let mut ratios = vec![1.0_f64; self.frames.len()];
        for note in &self.notes {
            if note.unvoiced_preserve || note.pitch_delta_semitones.abs() < 1e-4 {
                continue;
            }
            for f in note.start_frame..note.end_frame.min(self.frames.len()) {
                let Some(f0) = self.frames[f].f0_hz else {
                    continue;
                };
                let orig_midi = hz_to_midi(f0);
                let deviation = orig_midi - note.detected_midi;
                let target_midi =
                    note.target_midi() + deviation * note.pitch_modulation_scale.clamp(0.0, 2.0);
                let ratio = (midi_to_hz(target_midi) / f0).clamp(0.5, 2.0);
                ratios[f] = ratio;
            }
        }
        ratios
    }
}

pub trait PitchDetector: Send + Sync {
    fn info(&self) -> BackendInfo;
    fn detect(&self, samples: &[f64], sample_rate: f64) -> (Vec<PitchFrame>, usize);
}

pub trait NoteSegmenter: Send + Sync {
    fn info(&self) -> BackendInfo;
    fn segment(
        &self,
        frames: &[PitchFrame],
        hop_samples: usize,
        sample_rate: f64,
    ) -> Vec<EditableNote>;
}

pub trait PitchSynthesizer: Send + Sync {
    fn info(&self) -> BackendInfo;
    /// Resynthesize `span_samples` (`start_sample..end_sample`) from `source`
    /// according to `doc`'s note edits, leaving unvoiced/consonant spans intact.
    fn render_span(
        &self,
        source: &[f32],
        doc: &PitchAnalysisDoc,
        start_sample: usize,
        end_sample: usize,
    ) -> Vec<f32>;
}

/// Built-in classical YIN monophonic F0 detector (ported from `dsp::tune::detect`).
pub struct YinPitchDetector;

impl PitchDetector for YinPitchDetector {
    fn info(&self) -> BackendInfo {
        BackendInfo {
            id: "fts.pitch.yin".to_owned(),
            name: "FTS YIN".to_owned(),
            kind: "detector".to_owned(),
            description: "Classical YIN monophonic F0 tracker with aperiodicity gating and octave-glitch repair.".to_owned(),
        }
    }

    fn detect(&self, samples: &[f64], sample_rate: f64) -> (Vec<PitchFrame>, usize) {
        let mut yin = YinDetector::new(sample_rate, YinConfig::default());
        let window = yin.window();
        let hop = (window / 4).max(1);
        let mut frames = Vec::new();
        let mut pos = 0;
        while pos + window <= samples.len() {
            frames.push(yin.detect(&samples[pos..pos + window]));
            pos += hop;
        }
        // Confidence gate + 5-frame median octave-glitch repair.
        for f in &mut frames {
            if f.f0_hz.is_some() && f.aperiodicity > 0.35 {
                f.f0_hz = None;
            }
        }
        let midi: Vec<Option<f64>> = frames.iter().map(|f| f.f0_hz.map(hz_to_midi)).collect();
        for i in 0..frames.len() {
            let Some(m) = midi[i] else { continue };
            let mut hood: Vec<f64> = (i.saturating_sub(2)..(i + 3).min(midi.len()))
                .filter(|&j| j != i)
                .filter_map(|j| midi[j])
                .collect();
            if hood.len() < 2 {
                continue;
            }
            hood.sort_by(|a, b| a.total_cmp(b));
            let med = hood[hood.len() / 2];
            if (m - med).abs() > 7.0 {
                frames[i].f0_hz = Some(midi_to_hz(med));
            }
        }
        (frames, hop)
    }
}

/// Built-in contour-based note segmenter (ported from `dsp::tune::note`).
pub struct ContourNoteSegmenter;

impl NoteSegmenter for ContourNoteSegmenter {
    fn info(&self) -> BackendInfo {
        BackendInfo {
            id: "fts.segment.contour".to_owned(),
            name: "FTS Contour Segmenter".to_owned(),
            kind: "segmenter".to_owned(),
            description: "Groups stable voiced pitch frames into editable note blobs.".to_owned(),
        }
    }

    fn segment(
        &self,
        frames: &[PitchFrame],
        hop_samples: usize,
        sample_rate: f64,
    ) -> Vec<EditableNote> {
        let raw = segment_notes(frames, NoteConfig::default());
        let hop_s = hop_samples as f64 / sample_rate.max(1.0);
        raw.into_iter()
            .enumerate()
            .map(|(i, n)| EditableNote {
                id: format!("note#{i}"),
                start_frame: n.start_frame,
                end_frame: n.end_frame,
                start_s: n.start_frame as f64 * hop_s,
                end_s: n.end_frame as f64 * hop_s,
                detected_midi: n.median_midi,
                pitch_delta_semitones: 0.0,
                pitch_modulation_scale: 1.0,
                gain_db: 0.0,
                unvoiced_preserve: false,
            })
            .collect()
    }
}

/// Pitch-synchronous / overlap-add time-domain pitch synthesizer.
///
/// Resynthesizes voiced note regions at their target pitch ratio using
/// Hann-windowed pitch-synchronous overlap-add while passing unvoiced/breath/
/// untouched frames through bit-identically from the original recording.
pub struct TdOlaSynthesizer;

impl PitchSynthesizer for TdOlaSynthesizer {
    fn info(&self) -> BackendInfo {
        BackendInfo {
            id: "fts.synth.td-ola".to_owned(),
            name: "FTS Time-Domain OLA".to_owned(),
            kind: "synthesizer".to_owned(),
            description: "Pitch-synchronous time-domain overlap-add resynthesizer that leaves unvoiced/breath spans untouched.".to_owned(),
        }
    }

    fn render_span(
        &self,
        source: &[f32],
        doc: &PitchAnalysisDoc,
        start_sample: usize,
        end_sample: usize,
    ) -> Vec<f32> {
        let start = start_sample.min(source.len());
        let end = end_sample.min(source.len()).max(start);
        let len = end - start;
        if len == 0 {
            return Vec::new();
        }

        let ratios = doc.frame_shift_ratios();
        let hop = doc.hop_samples.max(1);
        let mut out = vec![0.0_f32; len];
        let mut weight = vec![0.0_f32; len];

        // Build pitch-synchronous analysis marks (glottal/waveform peaks spaced by
        // each voiced frame's fundamental period) so synthesis grains stay phase-locked.
        let mut marks: Vec<usize> = Vec::new();
        {
            let mut p = start;
            while p < end {
                let frame_idx = (p / hop).min(doc.frames.len().saturating_sub(1));
                let Some(f0) = doc.frames.get(frame_idx).and_then(|f| f.f0_hz) else {
                    p += 128;
                    continue;
                };
                if f0 <= 40.0 {
                    p += 128;
                    continue;
                }
                let period = (doc.sample_rate / f0).round().clamp(32.0, 2048.0) as usize;
                let lo = p.saturating_sub(period / 3);
                let hi = (p + period / 3).min(source.len().saturating_sub(1));
                let mut best = p.min(hi);
                let mut best_val = f32::NEG_INFINITY;
                for idx in lo..=hi {
                    if source[idx] > best_val {
                        best_val = source[idx];
                        best = idx;
                    }
                }
                if marks.last().is_none_or(|&prev| best > prev + period / 2) {
                    marks.push(best);
                    p = best + period;
                } else {
                    p += period;
                }
            }
        }

        let mut out_pos = start as f64;
        while (out_pos as usize) < end {
            let cur = out_pos.round() as usize;
            let frame_idx = (cur / hop).min(ratios.len().saturating_sub(1));
            let ratio = ratios.get(frame_idx).copied().unwrap_or(1.0);
            let f0_hz = doc.frames.get(frame_idx).and_then(|f| f.f0_hz);

            if (ratio - 1.0).abs() < 1e-4 || f0_hz.is_none() || marks.is_empty() {
                let step = 128_usize.min(end.saturating_sub(cur)).max(1);
                for k in 0..step {
                    let s_idx = cur + k;
                    if s_idx >= end {
                        break;
                    }
                    let o_idx = s_idx - start;
                    out[o_idx] += source[s_idx];
                    weight[o_idx] += 1.0;
                }
                out_pos += step as f64;
                continue;
            }

            let period = (doc.sample_rate / f0_hz.unwrap()).clamp(32.0, 2048.0);
            let target_period = (period / ratio).clamp(24.0, 4096.0);
            let half = period.round() as usize;
            let grain = (half * 2).max(64);

            // Nearest analysis pitch mark to `cur`.
            let mark_idx = marks.partition_point(|&m| m < cur);
            let an_center = match (mark_idx.checked_sub(1).and_then(|i| marks.get(i)), marks.get(mark_idx)) {
                (Some(&a), Some(&b)) => if cur.abs_diff(a) <= cur.abs_diff(b) { a } else { b },
                (Some(&a), None) => a,
                (None, Some(&b)) => b,
                (None, None) => cur,
            };

            for g in 0..grain {
                let offset = g as isize - half as isize;
                let s_idx = an_center as isize + offset;
                let o_idx = (cur - start) as isize + offset;
                if s_idx < 0
                    || (s_idx as usize) >= source.len()
                    || o_idx < 0
                    || (o_idx as usize) >= len
                {
                    continue;
                }
                let w = 0.5
                    * (1.0 - (core::f64::consts::TAU * (g as f64) / (grain as f64)).cos()) as f32;
                out[o_idx as usize] += source[s_idx as usize] * w;
                weight[o_idx as usize] += w;
            }
            out_pos += target_period;
        }

        for i in 0..len {
            if weight[i] > 1e-5 {
                out[i] /= weight[i];
            } else {
                out[i] = source[start + i];
            }
        }
        out
    }
}

pub struct PitchRegistry {
    detectors: Vec<Box<dyn PitchDetector>>,
    segmenters: Vec<Box<dyn NoteSegmenter>>,
    synthesizers: Vec<Box<dyn PitchSynthesizer>>,
}

impl Default for PitchRegistry {
    fn default() -> Self {
        let mut reg = Self {
            detectors: Vec::new(),
            segmenters: Vec::new(),
            synthesizers: Vec::new(),
        };
        reg.register_detector(Box::new(YinPitchDetector));
        reg.register_segmenter(Box::new(ContourNoteSegmenter));
        reg.register_synthesizer(Box::new(TdOlaSynthesizer));
        reg
    }
}

impl PitchRegistry {
    pub fn register_detector(&mut self, d: Box<dyn PitchDetector>) {
        let id = d.info().id;
        self.detectors.retain(|x| x.info().id != id);
        self.detectors.push(d);
    }

    pub fn register_segmenter(&mut self, s: Box<dyn NoteSegmenter>) {
        let id = s.info().id;
        self.segmenters.retain(|x| x.info().id != id);
        self.segmenters.push(s);
    }

    pub fn register_synthesizer(&mut self, s: Box<dyn PitchSynthesizer>) {
        let id = s.info().id;
        self.synthesizers.retain(|x| x.info().id != id);
        self.synthesizers.push(s);
    }

    pub fn detectors(&self) -> Vec<BackendInfo> {
        self.detectors.iter().map(|d| d.info()).collect()
    }

    pub fn segmenters(&self) -> Vec<BackendInfo> {
        self.segmenters.iter().map(|s| s.info()).collect()
    }

    pub fn synthesizers(&self) -> Vec<BackendInfo> {
        self.synthesizers.iter().map(|s| s.info()).collect()
    }

    pub fn detector(&self, id: &str) -> Option<&dyn PitchDetector> {
        self.detectors
            .iter()
            .find(|d| d.info().id == id)
            .map(|d| d.as_ref())
    }

    pub fn segmenter(&self, id: &str) -> Option<&dyn NoteSegmenter> {
        self.segmenters
            .iter()
            .find(|s| s.info().id == id)
            .map(|s| s.as_ref())
    }

    pub fn synthesizer(&self, id: &str) -> Option<&dyn PitchSynthesizer> {
        self.synthesizers
            .iter()
            .find(|s| s.info().id == id)
            .map(|s| s.as_ref())
    }

    pub fn analyze(
        &self,
        samples: &[f32],
        sample_rate: f64,
        detector_id: &str,
        segmenter_id: &str,
    ) -> Result<PitchAnalysisDoc, String> {
        let det = self
            .detector(detector_id)
            .ok_or_else(|| format!("unknown pitch detector: {detector_id}"))?;
        let seg = self
            .segmenter(segmenter_id)
            .ok_or_else(|| format!("unknown note segmenter: {segmenter_id}"))?;

        let f64_samples: Vec<f64> = samples.iter().map(|&s| f64::from(s)).collect();
        let (frames, hop) = det.detect(&f64_samples, sample_rate);
        let notes = seg.segment(&frames, hop, sample_rate);
        let hop_s = hop as f64 / sample_rate.max(1.0);
        let frame_records = frames
            .into_iter()
            .enumerate()
            .map(|(i, f)| FrameRecord {
                time_s: i as f64 * hop_s,
                f0_hz: f.f0_hz,
                aperiodicity: f.aperiodicity,
                rms: f.rms,
            })
            .collect();

        Ok(PitchAnalysisDoc {
            detector_id: detector_id.to_owned(),
            segmenter_id: segmenter_id.to_owned(),
            sample_rate,
            hop_samples: hop,
            total_samples: samples.len(),
            frames: frame_records,
            notes,
        })
    }
}
