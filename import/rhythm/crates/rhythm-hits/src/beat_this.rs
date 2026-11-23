use std::sync::Mutex;

use ort::{session::Session, value::Tensor};

use crate::models::{ModelManifest, resolve_cached_model};
use crate::onnx::{load_session, resample_mono};
use crate::registry::{BeatEvent, DetectorInfo, DetectorInput, DetectorOutput, HitDetector};

const ID: &str = "fts.hits.beat-this";
const COMBINED_ID: &str = "fts.hits.adtof-beat-this";
const BINS: usize = 128;
const BORDER: usize = 6;
const CHUNK: usize = 1500 - 2 * BORDER;

#[derive(Default)]
pub(crate) struct BeatThisDetector {
    sessions: Mutex<Option<(Session, Session)>>,
    drums: Option<crate::adtof::AdtofDetector>,
}

impl BeatThisDetector {
    pub(crate) fn combined() -> Self {
        Self {
            drums: Some(crate::adtof::AdtofDetector::bus()),
            ..Self::default()
        }
    }

    fn id(&self) -> &'static str {
        if self.drums.is_some() {
            COMBINED_ID
        } else {
            ID
        }
    }
}

impl HitDetector for BeatThisDetector {
    fn info(&self) -> DetectorInfo {
        DetectorInfo {
            id: self.id().into(),
            name: if self.drums.is_some() {
                "ADTOF + Beat This!"
            } else {
                "Beat This! small0"
            }
            .into(),
            description: "Beat and downbeat tracking from a mono drum_bus input.".into(),
            supports_triggers: false,
            supports_multi_mic_bleed: false,
            supports_beats: true,
            supports_sections: false,
        }
    }

    fn detect(&self, input: &DetectorInput<'_>) -> DetectorOutput {
        self.try_detect(input).unwrap_or_else(|error| {
            eprintln!("{error}");
            output(self.id(), input.sample_rate, Vec::new())
        })
    }

    fn try_detect(&self, input: &DetectorInput<'_>) -> Result<DetectorOutput, String> {
        let rate = input.sample_rate;
        if !rate.is_finite() || rate <= 0.0 {
            return Err("Beat This! requires a positive sample rate".into());
        }
        let bus = input
            .pieces
            .iter()
            .find(|p| p.name == "drum_bus")
            .ok_or("Beat This! requires a drum_bus piece")?
            .mic;
        if bus.is_empty() {
            return Ok(output(self.id(), rate, Vec::new()));
        }
        let duration = bus.len() as f64 / rate;
        let audio = resample_mono(bus, rate, 22_050.0);
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        if sessions.is_none() {
            let manifest = ModelManifest::from_json_str(include_str!("../../../models.json"))?;
            let load = |id| {
                let spec = manifest
                    .find(id)
                    .ok_or_else(|| format!("{id} missing from model manifest"))?;
                load_session(resolve_cached_model(spec, None)?)
            };
            *sessions = Some((load("beat-this-logmel")?, load("beat-this-small0")?));
        }
        let (frontend, tracker) = sessions.as_mut().unwrap();
        let waveform = Tensor::from_array(([1, audio.len()], audio)).map_err(|e| e.to_string())?;
        let mel = frontend
            .run(ort::inputs!["audio" => waveform])
            .map_err(|e| e.to_string())?;
        let (shape, values) = mel
            .get("mel")
            .ok_or("missing mel frontend output")?
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        if shape.len() != 2 || shape[1] != BINS as i64 {
            return Err(format!("unexpected mel shape {shape:?}"));
        }
        let (beats, downbeats) = track(tracker, values)?;
        drop(mel);
        drop(sessions);
        let beats = decode(&beats, &downbeats, duration);
        if let Some(drums) = &self.drums {
            let mut combined = drums.try_detect(input)?;
            combined.detector_id = self.id().into();
            combined.beats = beats;
            return Ok(combined);
        }
        Ok(output(self.id(), rate, beats))
    }
}

fn output(id: &str, sample_rate: f64, beats: Vec<BeatEvent>) -> DetectorOutput {
    DetectorOutput {
        detector_id: id.into(),
        sample_rate,
        beats,
        hits: Vec::new(),
        pieces: Vec::new(),
        sections: Vec::new(),
    }
}

fn track(tracker: &mut Session, mel: &[f32]) -> Result<(Vec<f32>, Vec<f32>), String> {
    let frames = mel.len() / BINS;
    let mut beats = Vec::with_capacity(frames);
    let mut downbeats = Vec::with_capacity(frames);
    for start in (0..frames).step_by(CHUNK) {
        let end = (start + CHUNK).min(frames);
        let left = start.saturating_sub(BORDER);
        let right = (end + BORDER).min(frames);
        let mut window = vec![0.0; (end - start + 2 * BORDER) * BINS];
        let offset = (BORDER - (start - left)) * BINS;
        window[offset..offset + (right - left) * BINS]
            .copy_from_slice(&mel[left * BINS..right * BINS]);
        let len = window.len() / BINS;
        let input = Tensor::from_array(([1, len, BINS], window)).map_err(|e| e.to_string())?;
        let logits = tracker
            .run(ort::inputs!["mel" => input])
            .map_err(|e| e.to_string())?;
        for (name, target) in [
            ("beat_logits", &mut beats),
            ("downbeat_logits", &mut downbeats),
        ] {
            let (shape, values) = logits
                .get(name)
                .ok_or_else(|| format!("missing {name} output"))?
                .try_extract_tensor::<f32>()
                .map_err(|e| e.to_string())?;
            if &shape[..] != [1, len as i64] {
                return Err(format!("unexpected {name} shape {shape:?}"));
            }
            target.extend_from_slice(&values[BORDER..BORDER + end - start]);
        }
    }
    Ok((beats, downbeats))
}

fn peak_frames(logits: &[f32]) -> impl Iterator<Item = usize> + '_ {
    (0..logits.len()).filter(|&i| {
        let score = logits[i];
        score > 0.0
            && logits[i.saturating_sub(3)..i].iter().all(|&x| score > x)
            && logits[i + 1..(i + 4).min(logits.len())]
                .iter()
                .all(|&x| score >= x)
    })
}

fn decode(beats: &[f32], downbeats: &[f32], duration: f64) -> Vec<BeatEvent> {
    let downbeat_frames: Vec<_> = peak_frames(downbeats).collect();
    peak_frames(beats)
        .filter_map(|frame| {
            let at = frame as f64 / 50.0;
            (at < duration).then(|| BeatEvent {
                at,
                downbeat: downbeat_frames
                    .iter()
                    .any(|&downbeat| frame.abs_diff(downbeat) <= 3),
                confidence: 1.0 / (1.0 + (-beats[frame]).exp()),
            })
        })
        .collect()
}
