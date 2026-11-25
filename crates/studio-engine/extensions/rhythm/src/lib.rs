//! Rhythm as an extension of Studio's engine (`rhythm`): an Architect
//! process extension ([`Rhythm`], served by `fts-rhythm` with no
//! arguments) that provides the DAW's editing contract (`daw.edit`,
//! `daw_edit`): `PLACE` is [`RHYTHM_PLACE`](place), where a performance's
//! hits play once moved. Its other actions (detection, planning, quantize,
//! rendering) run through [`dispatch`], which the CLI uses too.

use std::path::Path;

use rhythm_editing::{HitMove, HybridPlan, HybridPolicy, RecordedHit, plan_hybrid};
use rhythm_hits::{DetectorInput, DetectorOutput, DetectorRegistry, PieceInput};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

pub const MANIFEST: &str = include_str!("../extension.toml");

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PieceFileSpec {
    pub name: String,
    pub mic_wav: String,
    #[serde(default)]
    pub trigger_wav: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DetectRequest {
    #[serde(default = "default_detector")]
    pub detector: String,
    pub pieces: Vec<PieceFileSpec>,
    #[serde(default)]
    pub extra_mic_wavs: Vec<String>,
    #[serde(default)]
    pub threshold_db: Option<f64>,
    #[serde(default = "default_crest")]
    pub crest_db: f64,
    #[serde(default = "default_retrig")]
    pub retrig_ms: f64,
    #[serde(default)]
    pub tempo_bpm: Option<f64>,
    #[serde(default)]
    pub time_sig: Option<(u32, u32)>,
}

fn default_detector() -> String {
    "fts.hits.kit-fusion".to_owned()
}
fn default_crest() -> f64 {
    10.0
}
fn default_retrig() -> f64 {
    30.0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlaceHitsRequest {
    pub hits: Vec<RecordedHit>,
    #[serde(default)]
    pub moves: Vec<HitMove>,
    pub span: [f64; 2],
    #[serde(default)]
    pub policy: HybridPolicy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuantizePlanRequest {
    pub hits: Vec<RecordedHit>,
    pub bpm: f64,
    #[serde(default = "default_subdivision")]
    pub subdivision: u32,
    #[serde(default = "default_strength")]
    pub strength: f64,
    #[serde(default = "default_tolerance_ms")]
    pub tolerance_ms: f64,
    #[serde(default)]
    pub grid_origin_s: f64,
    #[serde(default = "default_group_window_ms")]
    pub group_window_ms: f64,
    #[serde(default)]
    pub pinned_moves: Vec<HitMove>,
    pub span: [f64; 2],
    #[serde(default)]
    pub policy: HybridPolicy,
}

fn default_group_window_ms() -> f64 {
    12.0
}

fn default_subdivision() -> u32 {
    4
}
fn default_strength() -> f64 {
    1.0
}
fn default_tolerance_ms() -> f64 {
    3.0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuantizePlanResponse {
    pub moves: Vec<HitMove>,
    pub plan: HybridPlan,
}

/// Read a mono (or downmixed) WAV file to `f32` samples + sample rate.
pub fn read_wav_mono(path: impl AsRef<Path>) -> Result<(Vec<f32>, f64), String> {
    let (raw, spec) = read_wav(path.as_ref())?;
    let ch = usize::from(spec.channels);
    let mono = if ch == 1 {
        raw
    } else {
        raw.chunks_exact(ch)
            .map(|frame| frame.iter().sum::<f32>() / ch as f32)
            .collect()
    };
    Ok((mono, f64::from(spec.sample_rate)))
}

fn read_wav(path: &Path) -> Result<(Vec<f32>, hound::WavSpec), String> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let spec = reader.spec();
    if spec.channels == 0 {
        return Err("WAV must have at least one channel".into());
    }
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
    Ok((raw, spec))
}

pub fn run_detect(
    registry: &DetectorRegistry,
    req: &DetectRequest,
) -> Result<DetectorOutput, String> {
    let detector = registry
        .get(&req.detector)
        .ok_or_else(|| format!("unknown detector: {}", req.detector))?;
    if req.pieces.is_empty() {
        return Err("at least one piece is required".to_owned());
    }

    let mut loaded: Vec<(String, Vec<f32>, Option<Vec<f32>>)> =
        Vec::with_capacity(req.pieces.len());
    let mut rate: Option<f64> = None;
    for p in &req.pieces {
        let (mic, sr) = read_wav_mono(&p.mic_wav)?;
        if let Some(prev) = rate {
            if (prev - sr).abs() > 1.0 {
                return Err(format!("sample rate mismatch: {prev} vs {sr}"));
            }
        } else {
            rate = Some(sr);
        }
        let trig = match &p.trigger_wav {
            Some(path) => Some(read_wav_mono(path)?.0),
            None => None,
        };
        loaded.push((p.name.clone(), mic, trig));
    }

    let mut extras_owned = Vec::with_capacity(req.extra_mic_wavs.len());
    for path in &req.extra_mic_wavs {
        extras_owned.push(read_wav_mono(path)?.0);
    }

    let pieces: Vec<PieceInput<'_>> = loaded
        .iter()
        .map(|(name, mic, trig)| PieceInput {
            name: name.clone(),
            mic: mic.as_slice(),
            trigger: trig.as_deref(),
        })
        .collect();
    let extras: Vec<&[f32]> = extras_owned.iter().map(Vec::as_slice).collect();

    detector.try_detect(&DetectorInput {
        sample_rate: rate.unwrap_or(48_000.0),
        pieces: &pieces,
        extra_mics: &extras,
        threshold_db: req.threshold_db,
        crest_db: req.crest_db,
        retrig_ms: req.retrig_ms,
        tempo_bpm: req.tempo_bpm,
        time_sig: req.time_sig,
    })
}

#[derive(Deserialize)]
struct RenderRequest {
    plan: HybridPlan,
    input_wav: std::path::PathBuf,
    output_wav: std::path::PathBuf,
}

fn render_wav(req: RenderRequest) -> Result<Json, String> {
    let (audio, mut spec) = read_wav(&req.input_wav)?;
    let rendered = rhythm_editing::render::render_group(
        &req.plan,
        &audio,
        usize::from(spec.channels),
        spec.sample_rate,
    )?;
    spec.sample_format = hound::SampleFormat::Float;
    spec.bits_per_sample = 32;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&req.output_wav)
        .map_err(|e| format!("create {}: {e}", req.output_wav.display()))?;
    let write = || -> Result<(), hound::Error> {
        let mut writer = hound::WavWriter::new(std::io::BufWriter::new(file), spec)?;
        for sample in &rendered {
            writer.write_sample(*sample)?;
        }
        writer.finalize()
    };
    if let Err(error) = write() {
        let _ = std::fs::remove_file(&req.output_wav);
        return Err(format!("write {}: {error}", req.output_wav.display()));
    }
    Ok(json!({
        "output_wav": req.output_wav,
        "channels": spec.channels,
        "sample_rate": spec.sample_rate,
        "frames": rendered.len() / usize::from(spec.channels),
    }))
}

/// `daw.edit`'s `PLACE`: where the pieces of the recording go once its
/// hits are moved, by rhythm's placement (`rhythm_editing::placement`).
///
/// # Errors
///
/// The request is not a `daw_edit::PlaceRequest`.
pub fn place(params: Json) -> Result<Json, String> {
    let request: daw_edit::PlaceRequest =
        serde_json::from_value(params).map_err(|e| format!("invalid PLACE request: {e}"))?;
    // rhythm_editing's placement types are daw_edit's, re-exported, so the
    // request's hits, cut, takes and fill pass straight through.
    let placed = rhythm_editing::placement::place_filled(
        &request.hits,
        request.span,
        request.cut,
        request.bounds,
        (&request.takes, request.fill),
    );
    serde_json::to_value(daw_edit::PlaceAnswer {
        pieces: placed.pieces,
        fills: placed.fills,
    })
    .map_err(|e| e.to_string())
}

/// Rhythm as an Architect extension: every action in its manifest.
pub struct Rhythm;

impl architect_extensions::linked::Extension for Rhythm {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut architect_extensions::linked::Builder) {
        ext.action("RHYTHM_PLACE", |args| async move { place(args) });
        for id in [
            "RHYTHM_STATE",
            "RHYTHM_DETECT",
            "RHYTHM_PLACE_HITS",
            "RHYTHM_QUANTIZE_PLAN",
            "RHYTHM_RENDER_PLAN",
        ] {
            ext.action(id, move |args| async move {
                dispatch(&DetectorRegistry::default(), id, args)
            });
        }
    }
}

pub fn dispatch(registry: &DetectorRegistry, action: &str, params: Json) -> Result<Json, String> {
    match action {
        "RHYTHM_PLACE" => place(params),
        "RHYTHM_STATE" => Ok(json!({
            "extension": "rhythm",
            "version": env!("CARGO_PKG_VERSION"),
            "detectors": registry.list(),
            "strategies": ["hybrid", "slip_only", "stretch_only"],
        })),
        "RHYTHM_DETECT" => {
            let req: DetectRequest =
                serde_json::from_value(params).map_err(|e| format!("invalid params: {e}"))?;
            let out = run_detect(registry, &req)?;
            serde_json::to_value(out).map_err(|e| e.to_string())
        }
        "RHYTHM_PLACE_HITS" => {
            let req: PlaceHitsRequest =
                serde_json::from_value(params).map_err(|e| format!("invalid params: {e}"))?;
            let plan = plan_hybrid(
                &req.hits,
                &req.moves,
                (req.span[0], req.span[1]),
                req.policy,
            );
            serde_json::to_value(plan).map_err(|e| e.to_string())
        }
        "RHYTHM_QUANTIZE_PLAN" => {
            let req: QuantizePlanRequest =
                serde_json::from_value(params).map_err(|e| format!("invalid params: {e}"))?;
            if !req.bpm.is_finite()
                || req.bpm <= 0.0
                || req.subdivision == 0
                || req.group_window_ms < 0.0
                || req.tolerance_ms < 0.0
            {
                return Err("invalid tempo, subdivision, or quantize window".into());
            }
            let moves = rhythm_editing::hybrid::quantize_grouped(
                &req.hits,
                rhythm_editing::hybrid::QuantizeGrid {
                    bpm: req.bpm,
                    subdivision: req.subdivision,
                    origin_s: req.grid_origin_s,
                    strength: req.strength.clamp(0.0, 1.0),
                    tolerance_s: req.tolerance_ms / 1000.0,
                    group_window_s: req.group_window_ms / 1000.0,
                },
                &req.pinned_moves,
            );
            let plan = plan_hybrid(&req.hits, &moves, (req.span[0], req.span[1]), req.policy);
            serde_json::to_value(QuantizePlanResponse { moves, plan }).map_err(|e| e.to_string())
        }
        "RHYTHM_RENDER_PLAN" => {
            let req = serde_json::from_value(params).map_err(|e| format!("invalid params: {e}"))?;
            render_wav(req)
        }
        other => Err(format!("unknown action: {other}")),
    }
}
