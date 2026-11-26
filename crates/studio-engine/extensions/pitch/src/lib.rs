//! Process extension core for `fts.pitch`.

use std::path::Path;

use pitch_core::{PitchAnalysisDoc, PitchRegistry, Scale};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

pub const MANIFEST: &str = include_str!("../extension.toml");

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnalyzeRequest {
    pub wav: String,
    #[serde(default = "default_detector")]
    pub detector: String,
    #[serde(default = "default_segmenter")]
    pub segmenter: String,
}

fn default_detector() -> String {
    "fts.pitch.yin".to_owned()
}
fn default_segmenter() -> String {
    "fts.segment.contour".to_owned()
}
fn default_synthesizer() -> String {
    "fts.synth.td-ola".to_owned()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CorrectScaleRequest {
    pub doc: PitchAnalysisDoc,
    #[serde(default)]
    pub root_pc: u8,
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_strength")]
    pub strength: f64,
}

fn default_mode() -> String {
    "major".to_owned()
}
fn default_strength() -> f64 {
    1.0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RenderSpanRequest {
    pub source_wav: String,
    pub doc: PitchAnalysisDoc,
    pub out_wav: String,
    #[serde(default)]
    pub span: Option<[f64; 2]>,
    #[serde(default = "default_synthesizer")]
    pub synthesizer: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RenderedCacheSpan {
    pub source_wav: String,
    pub rendered_wav: String,
    pub synthesizer_id: String,
    pub start_s: f64,
    pub end_s: f64,
    pub sample_rate: f64,
    pub frames: usize,
}

pub fn read_wav_mono(path: impl AsRef<Path>) -> Result<(Vec<f32>, f64), String> {
    let path = path.as_ref();
    let mut reader =
        hound::WavReader::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let spec = reader.spec();
    let ch = spec.channels.max(1) as usize;
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("read {}: {e}", path.display()))?,
        hound::SampleFormat::Int => {
            let scale = (1_i64 << (spec.bits_per_sample.saturating_sub(1))) as f32;
            reader
                .samples::<i32>()
                .map(|r| r.map(|v| (v as f32) / scale))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("read {}: {e}", path.display()))?
        }
    };
    let mono = if ch == 1 {
        raw
    } else {
        raw.chunks(ch)
            .map(|frame| frame.iter().copied().sum::<f32>() / (frame.len() as f32))
            .collect()
    };
    Ok((mono, f64::from(spec.sample_rate)))
}

pub fn write_wav_mono(path: impl AsRef<Path>, sr: u32, samples: &[f32]) -> Result<(), String> {
    let path = path.as_ref();
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .map_err(|e| format!("create {}: {e}", path.display()))?;
    for &s in samples {
        writer
            .write_sample(s)
            .map_err(|e| format!("write {}: {e}", path.display()))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("finalize {}: {e}", path.display()))
}

pub fn dispatch(registry: &PitchRegistry, action: &str, params: Json) -> Result<Json, String> {
    match action {
        "PITCH_STATE" => Ok(json!({
            "extension": "fts.pitch",
            "version": env!("CARGO_PKG_VERSION"),
            "detectors": registry.detectors(),
            "segmenters": registry.segmenters(),
            "synthesizers": registry.synthesizers(),
        })),
        "PITCH_ANALYZE" => {
            let req: AnalyzeRequest =
                serde_json::from_value(params).map_err(|e| format!("invalid params: {e}"))?;
            let (samples, sr) = read_wav_mono(&req.wav)?;
            let doc = registry.analyze(&samples, sr, &req.detector, &req.segmenter)?;
            serde_json::to_value(doc).map_err(|e| e.to_string())
        }
        "PITCH_CORRECT_SCALE" => {
            let mut req: CorrectScaleRequest =
                serde_json::from_value(params).map_err(|e| format!("invalid params: {e}"))?;
            let scale = match req.mode.to_ascii_lowercase().as_str() {
                "minor" => Scale::minor(req.root_pc % 12),
                _ => Scale::major(req.root_pc % 12),
            };
            let strength = req.strength.clamp(0.0, 1.0);
            for note in &mut req.doc.notes {
                if note.unvoiced_preserve {
                    continue;
                }
                let snapped = scale.snap(note.detected_midi);
                note.pitch_delta_semitones = (snapped - note.detected_midi) * strength;
            }
            serde_json::to_value(req.doc).map_err(|e| e.to_string())
        }
        "PITCH_RENDER_SPAN" => {
            let req: RenderSpanRequest =
                serde_json::from_value(params).map_err(|e| format!("invalid params: {e}"))?;
            let synth = registry
                .synthesizer(&req.synthesizer)
                .ok_or_else(|| format!("unknown synthesizer: {}", req.synthesizer))?;
            let (samples, sr) = read_wav_mono(&req.source_wav)?;
            let (start_s, end_s) = match req.span {
                Some([a, b]) => (a.max(0.0), b.max(a.max(0.0))),
                None => (0.0, samples.len() as f64 / sr.max(1.0)),
            };
            let start_idx = ((start_s * sr).round() as usize).min(samples.len());
            let end_idx = ((end_s * sr).round() as usize)
                .min(samples.len())
                .max(start_idx);
            let rendered = synth.render_span(&samples, &req.doc, start_idx, end_idx);
            write_wav_mono(&req.out_wav, sr.round() as u32, &rendered)?;
            let desc = RenderedCacheSpan {
                source_wav: req.source_wav,
                rendered_wav: req.out_wav,
                synthesizer_id: req.synthesizer,
                start_s: start_idx as f64 / sr.max(1.0),
                end_s: end_idx as f64 / sr.max(1.0),
                sample_rate: sr,
                frames: rendered.len(),
            };
            serde_json::to_value(desc).map_err(|e| e.to_string())
        }
        other => Err(format!("unknown action: {other}")),
    }
}
