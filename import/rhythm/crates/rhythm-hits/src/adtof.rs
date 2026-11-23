use std::collections::HashSet;
use std::sync::Mutex;

use ort::{session::Session, value::Tensor};

use crate::gate::{Gate, gate};
use crate::models::{ModelManifest, resolve_cached_model};
use crate::registry::{
    DetectedHit, DetectorInfo, DetectorInput, DetectorOutput, HitDetector, PieceSummary,
};
use crate::{attack_peak, attack_start, db, samples, seconds};

const CLASSES: [&str; 5] = ["kick", "snare", "toms", "hihat", "cymbals"];
const CLOSE_MICS: [(&str, usize); 6] = [
    ("kick", 0),
    ("snare", 1),
    ("t1", 2),
    ("t2", 2),
    ("t3", 2),
    ("t4", 2),
];

#[derive(Clone, Copy)]
enum Timing {
    Raw,
    Bus,
    Overheads,
    CloseMics,
}

pub(crate) struct AdtofDetector {
    timing: Timing,
    session: Mutex<Option<Session>>,
}

impl AdtofDetector {
    pub(crate) fn variants() -> [Self; 4] {
        [
            Timing::Raw,
            Timing::Bus,
            Timing::Overheads,
            Timing::CloseMics,
        ]
        .map(|timing| Self {
            timing,
            session: Mutex::new(None),
        })
    }

    pub(crate) fn bus() -> Self {
        Self {
            timing: Timing::Bus,
            session: Mutex::new(None),
        }
    }

    fn activations(&self, audio: Vec<f32>) -> Result<Vec<f32>, String> {
        let mut session = self.session.lock().map_err(|e| e.to_string())?;
        if session.is_none() {
            let manifest = ModelManifest::from_json_str(include_str!("../../../models.json"))?;
            let spec = manifest
                .find("adtof-frame-rnn")
                .ok_or("ADTOF missing from model manifest")?;
            *session = Some(crate::onnx::load_session(resolve_cached_model(
                spec, None,
            )?)?);
        }
        let input = Tensor::from_array(([1, audio.len()], audio)).map_err(|e| e.to_string())?;
        let outputs = session
            .as_mut()
            .unwrap()
            .run(ort::inputs!["waveform" => input])
            .map_err(|e| format!("ADTOF inference: {e}"))?;
        let (shape, values) = outputs
            .get("activations")
            .ok_or("ADTOF missing activations output")?
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        if shape.len() != 3 || shape[0] != 1 || shape[2] != 5 {
            return Err(format!("unexpected ADTOF output shape {shape:?}"));
        }
        Ok(values.to_vec())
    }

    fn empty_output(&self, sample_rate: f64) -> DetectorOutput {
        DetectorOutput {
            detector_id: self.info().id,
            sample_rate,
            pieces: Vec::new(),
            hits: Vec::new(),
            beats: Vec::new(),
            sections: Vec::new(),
        }
    }
}

impl HitDetector for AdtofDetector {
    fn info(&self) -> DetectorInfo {
        let (id, name) = match self.timing {
            Timing::Raw => ("fts.hits.adtof-onnx-raw", "ADTOF Frame RNN (raw)"),
            Timing::Bus => ("fts.hits.adtof-onnx", "ADTOF Frame RNN (bus snap)"),
            Timing::Overheads => ("fts.hits.adtof-onnx-ohs", "ADTOF Frame RNN (overhead snap)"),
            Timing::CloseMics => (
                "fts.hits.adtof-onnx-close",
                "ADTOF Frame RNN (close mic gates)",
            ),
        };
        DetectorInfo {
            id: id.into(),
            name: name.into(),
            description: "Five-class drum transcription from a mono drum_bus, or independent hit detection on named kick, snare, and tom tracks.".into(),
            supports_triggers: false,
            supports_multi_mic_bleed: matches!(self.timing, Timing::CloseMics),
            supports_beats: false,
            supports_sections: false,
        }
    }

    fn detect(&self, input: &DetectorInput<'_>) -> DetectorOutput {
        self.try_detect(input).unwrap_or_else(|error| {
            eprintln!("{error}");
            self.empty_output(input.sample_rate)
        })
    }

    fn try_detect(&self, input: &DetectorInput<'_>) -> Result<DetectorOutput, String> {
        let rate = input.sample_rate;
        if !rate.is_finite() || rate <= 0.0 {
            return Err("ADTOF requires a positive sample rate".into());
        }
        let mic = |name| input.pieces.iter().find(|p| p.name == name).map(|p| p.mic);
        let bus = mic("drum_bus");
        let close_only = bus.is_none() && matches!(self.timing, Timing::Raw | Timing::Bus);
        if close_only && input.pieces.len() > 1 {
            let mut output = self.empty_output(rate);
            let mut names = HashSet::new();
            for piece in input.pieces {
                let name = piece.name.trim().to_ascii_lowercase();
                if !names.insert(name) {
                    return Err(format!("duplicate ADTOF close-mic piece '{}'", piece.name));
                }
                let mut single = self.try_detect(&DetectorInput {
                    pieces: std::slice::from_ref(piece),
                    ..*input
                })?;
                output.hits.append(&mut single.hits);
                output.pieces.append(&mut single.pieces);
            }
            output
                .hits
                .sort_by(|a, b| a.at.total_cmp(&b.at).then(a.id.cmp(&b.id)));
            return Ok(output);
        }
        let solo = if close_only {
            input.pieces.first().and_then(|piece| {
                CLOSE_MICS
                    .iter()
                    .find(|(name, _)| piece.name.trim().eq_ignore_ascii_case(name))
                    .map(|&(name, class)| (name, class, piece.mic))
            })
        } else {
            None
        };
        let source = bus
            .or_else(|| solo.map(|(_, _, signal)| signal))
            .ok_or("ADTOF requires drum_bus or named kick, snare, t1–t4 close-mic pieces")?;
        let reference = match self.timing {
            Timing::Overheads => mic("ohs").ok_or("ADTOF overhead timing requires an ohs piece")?,
            _ => source,
        };
        let mut output = self.empty_output(rate);
        if source.is_empty() {
            return Ok(output);
        }
        let mut audio = crate::onnx::resample_mono(source, rate, 44_100.0);
        let peak = audio
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        if peak == 0.0 {
            return Ok(output);
        }
        // Comped drum buses can have substantial headroom. Limit the gain
        // on near-silence while keeping waveform timing at the original level.
        let gain = (0.95 / peak).min(32.0);
        for sample in &mut audio {
            *sample *= gain;
        }
        let activations = self.activations(audio)?;
        let mut close_mics = Vec::new();
        if matches!(self.timing, Timing::CloseMics) {
            let settings = Gate {
                threshold_db: input.threshold_db.unwrap_or(-32.0),
                crest_db: input.crest_db,
                retrig_secs: input.retrig_ms / 1000.0,
            };
            for (name, class) in CLOSE_MICS {
                if let Some(signal) = mic(name) {
                    close_mics.push((name, class, signal, gate(signal, rate, &settings)));
                }
            }
            if close_mics.is_empty() {
                return Err("ADTOF close-mic timing requires kick, snare or tom pieces".into());
            }
        }
        for (class, name) in CLASSES.into_iter().enumerate() {
            if solo.is_some_and(|(_, selected, _)| selected != class) {
                continue;
            }
            let mut seen = HashSet::new();
            for (frame, confidence) in peaks(&activations, class) {
                let nominal = samples(frame as f64 / 100.0, rate);
                if nominal >= source.len() {
                    continue;
                }
                let mut signal = reference;
                let mut at = nominal;
                let mut piece = solo.map_or(name, |(piece, _, _)| piece);
                if matches!(self.timing, Timing::CloseMics) && class < 3 {
                    let Some((close_name, close_signal, candidate)) =
                        nearest_gate(&close_mics, class, nominal, samples(0.025, rate))
                    else {
                        continue;
                    };
                    piece = close_name;
                    signal = close_signal;
                    at = candidate;
                }
                if !matches!(self.timing, Timing::Raw) {
                    at = attack_start(signal, rate, at);
                }
                if !seen.insert((piece, at)) {
                    continue;
                }
                output.hits.push(DetectedHit {
                    id: format!("{piece}#{}", output.hits.len()),
                    piece: piece.into(),
                    at: seconds(at, rate),
                    peak_db: db(f64::from(attack_peak(signal, rate, at))),
                    confidence,
                    ghost: false,
                    flam_partner: None,
                    bleed_suspect: false,
                });
            }
        }
        for hit in &output.hits {
            if let Some(summary) = output.pieces.iter_mut().find(|s| s.piece == hit.piece) {
                summary.hit_count += 1;
            } else {
                output.pieces.push(PieceSummary {
                    piece: hit.piece.clone(),
                    method: "adtof-frame-rnn".into(),
                    hit_count: 1,
                });
            }
        }
        output.hits.sort_by(|a, b| a.at.total_cmp(&b.at));
        Ok(output)
    }
}

fn peaks(values: &[f32], class: usize) -> impl Iterator<Item = (usize, f32)> {
    let column = values.get(class..).unwrap_or_default();
    column
        .iter()
        .step_by(5)
        .copied()
        .enumerate()
        .filter(move |&(i, v)| {
            let prev = i
                .checked_sub(1)
                .and_then(|j| column.get(j * 5))
                .copied()
                .unwrap_or(0.0);
            let next = column.get((i + 1) * 5).copied().unwrap_or(0.0);
            v >= [0.22, 0.24, 0.32, 0.22, 0.30][class] && v > prev && v >= next
        })
}

fn nearest_gate<'a>(
    mics: &[(&'a str, usize, &'a [f32], Vec<usize>)],
    class: usize,
    nominal: usize,
    window: usize,
) -> Option<(&'a str, &'a [f32], usize)> {
    mics.iter()
        .filter(|mic| mic.1 == class)
        .flat_map(|(name, _, signal, gates)| {
            let split = gates.partition_point(|at| *at < nominal);
            let start = split.saturating_sub(1);
            let end = (split + 1).min(gates.len());
            gates[start..end]
                .iter()
                .map(move |&at| (*name, *signal, at))
        })
        .filter(|(_, _, at)| at.abs_diff(nominal) <= window)
        .min_by_key(|(_, _, at)| at.abs_diff(nominal))
}

/// Keep calibrated close-mic hits and recover missing kick/snare hits from the bus.
pub(crate) struct AdtofDspDetector {
    neural: AdtofDetector,
}

impl Default for AdtofDspDetector {
    fn default() -> Self {
        Self {
            neural: AdtofDetector::bus(),
        }
    }
}

impl HitDetector for AdtofDspDetector {
    fn info(&self) -> DetectorInfo {
        let mut info = crate::registry::KitFusionDetector.info();
        info.id = "fts.hits.adtof-onnx-dsp".into();
        info.name = "ADTOF + Kit Fusion".into();
        info.description = "Calibrated close-mic hits with neural kick and snare recovery.".into();
        info
    }

    fn detect(&self, input: &DetectorInput<'_>) -> DetectorOutput {
        self.try_detect(input).unwrap_or_else(|error| {
            eprintln!("{error}");
            self.neural.empty_output(input.sample_rate)
        })
    }

    fn try_detect(&self, input: &DetectorInput<'_>) -> Result<DetectorOutput, String> {
        if !input.pieces.iter().any(|p| p.name == "drum_bus") {
            return Err("ADTOF + Kit Fusion requires drum_bus and named close mics".into());
        }
        let pieces: Vec<_> = input
            .pieces
            .iter()
            .filter(|piece| CLOSE_MICS.iter().any(|(name, _)| *name == piece.name))
            .map(|piece| crate::registry::PieceInput {
                name: piece.name.clone(),
                mic: piece.mic,
                trigger: piece.trigger,
            })
            .collect();
        if pieces.is_empty() {
            return Err("ADTOF + Kit Fusion requires at least one close mic".into());
        }

        let mut output = crate::registry::KitFusionDetector.try_detect(&DetectorInput {
            pieces: &pieces,
            ..*input
        })?;
        let neural = self.neural.try_detect(input)?;
        for (index, mut hit) in neural.hits.into_iter().enumerate() {
            if !matches!(hit.piece.as_str(), "kick" | "snare") {
                continue;
            }
            let Some(piece) = pieces.iter().find(|piece| piece.name == hit.piece) else {
                continue;
            };
            if output
                .pieces
                .iter()
                .any(|summary| summary.piece == hit.piece && summary.method == "trigger+mic")
            {
                continue;
            }
            let nominal = samples(hit.at, input.sample_rate);
            if nominal >= piece.mic.len() {
                continue;
            }
            let at = attack_start(piece.mic, input.sample_rate, nominal);
            hit.at = seconds(at, input.sample_rate);
            if output.hits.iter().any(|existing| {
                existing.piece == hit.piece && (existing.at - hit.at).abs() <= 0.020
            }) {
                continue;
            }
            hit.id = format!("adtof:{}#{index}", hit.piece);
            hit.peak_db = db(f64::from(attack_peak(piece.mic, input.sample_rate, at)));
            if let Some(summary) = output.pieces.iter_mut().find(|s| s.piece == hit.piece) {
                summary.hit_count += 1;
                summary.method = "kit-fusion+adtof".into();
            }
            output.hits.push(hit);
        }
        output
            .hits
            .sort_by(|a, b| a.at.total_cmp(&b.at).then(a.id.cmp(&b.id)));
        output.detector_id = self.info().id;
        Ok(output)
    }
}
