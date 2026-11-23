//! Swappable hit-detector trait and registry.
//!
//! Mirrors the DAW's capability-based provider contract: callers ask for a
//! detector by stable ID (e.g. `fts.hits.kit-fusion` or `fts.hits.envelope-gate`)
//! and get back canonical [`DetectedHit`] records plus optional beat, downbeat,
//! and song-section analyses.

use serde::{Deserialize, Serialize};

use crate::attack::{attack_peak, attack_start};
use crate::fuse::{Hit, pair_flams, settle};
use crate::gate::{Gate, gate};
use crate::kit::{Method, Piece, Settings, Trust, detect_kit};
use crate::{db, seconds};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DetectorInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub supports_triggers: bool,
    pub supports_multi_mic_bleed: bool,
    #[serde(default)]
    pub supports_beats: bool,
    #[serde(default)]
    pub supports_sections: bool,
}

pub struct PieceInput<'a> {
    pub name: String,
    pub mic: &'a [f32],
    pub trigger: Option<&'a [f32]>,
}

pub struct DetectorInput<'a> {
    pub sample_rate: f64,
    pub pieces: &'a [PieceInput<'a>],
    pub extra_mics: &'a [&'a [f32]],
    pub threshold_db: Option<f64>,
    pub crest_db: f64,
    pub retrig_ms: f64,
    /// Optional known DAW session tempo in BPM (prior for metrical/BPM-conditioned models).
    pub tempo_bpm: Option<f64>,
    /// Optional known time signature `(numerator, denominator)` e.g. `(4, 4)`.
    pub time_sig: Option<(u32, u32)>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DetectedHit {
    pub id: String,
    pub piece: String,
    /// Attack start time in source seconds.
    pub at: f64,
    /// Attack peak on the close mic in dBFS.
    pub peak_db: f64,
    pub confidence: f32,
    pub ghost: bool,
    pub flam_partner: Option<String>,
    pub bleed_suspect: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BeatEvent {
    /// Beat time in source seconds.
    pub at: f64,
    /// True if this beat is bar downbeat (beat 1).
    pub downbeat: bool,
    pub confidence: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SectionEvent {
    pub start: f64,
    pub end: f64,
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PieceSummary {
    pub piece: String,
    pub method: String,
    pub hit_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DetectorOutput {
    pub detector_id: String,
    pub sample_rate: f64,
    pub pieces: Vec<PieceSummary>,
    pub hits: Vec<DetectedHit>,
    #[serde(default)]
    pub beats: Vec<BeatEvent>,
    #[serde(default)]
    pub sections: Vec<SectionEvent>,
}

pub trait HitDetector: Send + Sync {
    fn info(&self) -> DetectorInfo;
    fn detect(&self, input: &DetectorInput<'_>) -> DetectorOutput;
}

/// Full multi-mic calibrated kit detector (`detect_kit`).
pub struct KitFusionDetector;

impl HitDetector for KitFusionDetector {
    fn info(&self) -> DetectorInfo {
        DetectorInfo {
            id: "fts.hits.kit-fusion".to_owned(),
            name: "FTS Kit Fusion".to_owned(),
            description: "Multi-mic drum kit detector with trigger health checks, automatic close-mic calibration, and cross-mic bleed rejection.".to_owned(),
            supports_triggers: true,
            supports_multi_mic_bleed: true,
            supports_beats: false,
            supports_sections: false,
        }
    }

    fn detect(&self, input: &DetectorInput<'_>) -> DetectorOutput {
        let kit_pieces: Vec<Piece<'_>> = input
            .pieces
            .iter()
            .map(|p| Piece {
                mic: p.mic,
                trigger: p.trigger,
            })
            .collect();
        let settings = Settings {
            trust: Trust::Auto,
            gate: input.threshold_db.map(|threshold_db| Gate {
                threshold_db,
                crest_db: input.crest_db,
                retrig_secs: input.retrig_ms / 1000.0,
            }),
            crest_db: input.crest_db,
            retrig_secs: input.retrig_ms / 1000.0,
            trigger_under_db: 28.0,
        };
        let found = detect_kit(&kit_pieces, input.extra_mics, input.sample_rate, &settings);
        let mut summaries = Vec::with_capacity(found.len());
        let mut all_hits = Vec::new();
        for (pi, f) in found.into_iter().enumerate() {
            let piece_name = input.pieces[pi].name.clone();
            let method = match f.method {
                Method::Fused => "trigger+mic",
                Method::MicAlone => "mic",
            };
            summaries.push(PieceSummary {
                piece: piece_name.clone(),
                method: method.to_owned(),
                hit_count: f.hits.len(),
            });
            for (hi, h) in f.hits.iter().enumerate() {
                let id = format!("{piece_name}#{hi}");
                let flam_partner = h.flam_with.map(|partner| format!("{piece_name}#{partner}"));
                all_hits.push(DetectedHit {
                    id,
                    piece: piece_name.clone(),
                    at: seconds(h.at, input.sample_rate),
                    peak_db: db(f64::from(h.peak)),
                    confidence: h.confidence as f32,
                    ghost: h.ghost,
                    flam_partner,
                    bleed_suspect: f.method == Method::MicAlone && h.confidence < 0.6,
                });
            }
        }
        all_hits.sort_by(|a, b| a.at.total_cmp(&b.at).then(a.id.cmp(&b.id)));
        DetectorOutput {
            detector_id: self.info().id,
            sample_rate: input.sample_rate,
            pieces: summaries,
            hits: all_hits,
            beats: Vec::new(),
            sections: Vec::new(),
        }
    }
}

/// Simple fast per-channel envelope + crest gate detector (no trigger/bleed fusion).
pub struct EnvelopeGateDetector;

impl HitDetector for EnvelopeGateDetector {
    fn info(&self) -> DetectorInfo {
        DetectorInfo {
            id: "fts.hits.envelope-gate".to_owned(),
            name: "FTS Envelope Gate".to_owned(),
            description: "Fast single-channel envelope/crest transient gate without cross-mic bleed suppression.".to_owned(),
            supports_triggers: false,
            supports_multi_mic_bleed: false,
            supports_beats: false,
            supports_sections: false,
        }
    }

    fn detect(&self, input: &DetectorInput<'_>) -> DetectorOutput {
        let g = Gate {
            threshold_db: input.threshold_db.unwrap_or(-32.0),
            crest_db: input.crest_db,
            retrig_secs: input.retrig_ms / 1000.0,
        };
        let mut summaries = Vec::with_capacity(input.pieces.len());
        let mut all_hits = Vec::new();
        for p in input.pieces {
            let raw = gate(p.mic, input.sample_rate, &g);
            let mut hits: Vec<Hit> = raw
                .into_iter()
                .map(|at| {
                    let refined = attack_start(p.mic, input.sample_rate, at);
                    let peak = attack_peak(p.mic, input.sample_rate, refined);
                    Hit {
                        at: refined,
                        peak,
                        confidence: 0.75,
                        from_trigger: false,
                        from_mic: true,
                        ghost: false,
                        flam_with: None,
                    }
                })
                .collect();
            hits = settle(hits, input.sample_rate);
            pair_flams(&mut hits, input.sample_rate);
            summaries.push(PieceSummary {
                piece: p.name.clone(),
                method: "envelope-gate".to_owned(),
                hit_count: hits.len(),
            });
            for (hi, h) in hits.iter().enumerate() {
                let id = format!("{}#{hi}", p.name);
                let flam_partner = h.flam_with.map(|partner| format!("{}#{partner}", p.name));
                all_hits.push(DetectedHit {
                    id,
                    piece: p.name.clone(),
                    at: seconds(h.at, input.sample_rate),
                    peak_db: db(f64::from(h.peak)),
                    confidence: h.confidence as f32,
                    ghost: h.ghost,
                    flam_partner,
                    bleed_suspect: false,
                });
            }
        }
        all_hits.sort_by(|a, b| a.at.total_cmp(&b.at).then(a.id.cmp(&b.id)));
        DetectorOutput {
            detector_id: self.info().id,
            sample_rate: input.sample_rate,
            pieces: summaries,
            hits: all_hits,
            beats: Vec::new(),
            sections: Vec::new(),
        }
    }
}

pub struct DetectorRegistry {
    detectors: Vec<Box<dyn HitDetector>>,
}

impl Default for DetectorRegistry {
    fn default() -> Self {
        let mut reg = Self {
            detectors: Vec::new(),
        };
        reg.register(Box::new(KitFusionDetector));
        reg.register(Box::new(EnvelopeGateDetector));
        reg
    }
}

impl DetectorRegistry {
    pub fn register(&mut self, detector: Box<dyn HitDetector>) {
        let id = detector.info().id;
        self.detectors.retain(|d| d.info().id != id);
        self.detectors.push(detector);
    }

    pub fn list(&self) -> Vec<DetectorInfo> {
        self.detectors.iter().map(|d| d.info()).collect()
    }

    pub fn get(&self, id: &str) -> Option<&dyn HitDetector> {
        self.detectors
            .iter()
            .find(|d| d.info().id == id)
            .map(|d| d.as_ref())
    }
}
