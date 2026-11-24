//! Rhythm as an extension of Studio's engine (`rhythm`): an Architect
//! process extension ([`Rhythm`], served by `fts-rhythm` with no
//! arguments) that provides the DAW's editing contract (`daw.edit`,
//! `daw_edit`): `PLACE` is [`RHYTHM_PLACE`](place), where a performance's
//! hits play once moved, and `PLAN` is [`RHYTHM_PLAN`](plan), where a
//! kit's played hits should go (`rhythm_editing::plan`). Its other actions (detection, planning, quantize,
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
/// hits are moved, each gap filled by rhythm's rule
/// (`rhythm_editing::placement`: the piece before plays on, the piece after
/// reaches back, they crossfade, only what is left is stretched, never a
/// hit moved), and what was done at each gap.
///
/// # Errors
///
/// The request is not a `daw_edit::PlaceRequest`.
pub fn place(params: Json) -> Result<Json, String> {
    let span = tracing::info_span!(
        "RHYTHM_PLACE",
        hits = tracing::field::Empty,
        takes = tracing::field::Empty,
        pieces = tracing::field::Empty,
        "fill.gaps" = tracing::field::Empty,
        "fill.extended" = tracing::field::Empty,
        "fill.reached_back" = tracing::field::Empty,
        "fill.crossfaded" = tracing::field::Empty,
        "fill.stretched" = tracing::field::Empty,
        "fill.slowest" = tracing::field::Empty,
        "fill.left" = tracing::field::Empty,
        "fill.take_stopped" = tracing::field::Empty,
        "fill.take_crossed" = tracing::field::Empty,
    );
    let _in = span.enter();
    let request: daw_edit::PlaceRequest =
        serde_json::from_value(params).map_err(|e| format!("invalid PLACE request: {e}"))?;
    span.record("hits", request.hits.len());
    span.record("takes", request.takes.len());
    let placed = rhythm_editing::placement::place_filled(
        &request.hits,
        request.span,
        request.cut,
        request.bounds,
        (&request.takes, request.fill),
    );
    let fills = &placed.fills;
    let count =
        |f: &dyn Fn(&daw_edit::placement::Filled) -> bool| fills.iter().filter(|x| f(x)).count();
    span.record("pieces", placed.pieces.len());
    span.record("fill.gaps", fills.len());
    span.record("fill.extended", count(&|f| f.extended > 0.0));
    span.record("fill.reached_back", count(&|f| f.reached_back > 0.0));
    span.record("fill.crossfaded", count(&|f| f.crossfade > 0.0));
    span.record("fill.stretched", count(&|f| f.stretched.is_some()));
    span.record("fill.left", count(&|f| f.left > 0.0));
    span.record(
        "fill.take_stopped",
        count(&|f| f.take_switch == daw_edit::placement::TakeSwitch::Stopped),
    );
    span.record(
        "fill.take_crossed",
        count(&|f| f.take_switch == daw_edit::placement::TakeSwitch::Crossed),
    );
    if let Some(slowest) = fills
        .iter()
        .filter_map(|f| f.stretched.map(|s| s.rate))
        .reduce(f64::min)
    {
        span.record("fill.slowest", slowest);
    }
    serde_json::to_value(daw_edit::PlaceAnswer {
        pieces: placed.pieces,
        fills: placed.fills,
    })
    .map_err(|e| e.to_string())
}

/// `daw.edit`'s `PLAN`: a target for every played hit of a kit, with why,
/// the rides and removes, and the doubts (`rhythm_editing::plan`).
///
/// # Errors
///
/// The request is not a `daw_edit::PlanRequest`.
pub fn plan(params: Json) -> Result<Json, String> {
    let span = tracing::info_span!(
        "RHYTHM_PLAN",
        hits = tracing::field::Empty,
        ms = tracing::field::Empty
    );
    let _in = span.enter();
    let request: daw_edit::PlanRequest =
        serde_json::from_value(params).map_err(|e| format!("invalid PLAN request: {e}"))?;
    span.record("hits", request.hits.len());
    let started = std::time::Instant::now();
    let answer = rhythm_editing::plan::plan(&request);
    span.record("ms", started.elapsed().as_secs_f64() * 1000.0);
    serde_json::to_value(answer).map_err(|e| e.to_string())
}

/// Rhythm as an Architect extension: every action in its manifest.
pub struct Rhythm;

impl architect_extensions::linked::Extension for Rhythm {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut architect_extensions::linked::Builder) {
        ext.action("RHYTHM_PLACE", |args| async move { place(args) });
        ext.action("RHYTHM_PLAN", |args| async move { plan(args) });
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
        "RHYTHM_PLAN" => plan(params),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_answers_daw_edits_plan() {
        let request = json!({
            "hits": [
                {"id": "k", "piece": "Kick", "time": 1.51},
                {"id": "s", "piece": "Snare", "time": 1.889}
            ],
            "bars": [{"bar": 1, "at": 1.5, "beats": 4, "unit": 4, "beat": 0.375}]
        });
        let out = dispatch(&DetectorRegistry::default(), "RHYTHM_PLAN", request).unwrap();
        let answer: daw_edit::PlanAnswer = serde_json::from_value(out).unwrap();
        assert!((answer.targets["k"] - 1.5).abs() < 1e-9);
        assert!((answer.targets["s"] - 1.875).abs() < 1e-9);
        assert!(
            dispatch(
                &DetectorRegistry::default(),
                "RHYTHM_PLAN",
                json!({"hits": 3})
            )
            .is_err()
        );
    }

    #[test]
    fn the_manifest_provides_plan() {
        assert!(MANIFEST.contains("id = \"RHYTHM_PLAN\""));
        assert!(MANIFEST.contains(&format!("\"daw.edit\" = \"{}\"", daw_edit::VERSION)));
    }

    #[test]
    fn place_answers_with_its_fills() {
        // The snare 330 ms later after 300: reached back, the last of the
        // kick's decay stretched at 0.8, said in `fills`.
        let request = json!({
            "hits": [{"at": 1.0, "shift": 0.0}, {"at": 1.3, "shift": 0.33}],
            "span": [0.995, 2.5],
            "takes": [{"from": 0.0, "to": 3.0}]
        });
        let out = dispatch(&DetectorRegistry::default(), "RHYTHM_PLACE", request).unwrap();
        let answer: daw_edit::PlaceAnswer = serde_json::from_value(out).unwrap();
        assert_eq!(answer.pieces.len(), 2);
        let [fill] = answer.fills.as_slice() else {
            panic!("{:?}", answer.fills)
        };
        assert!((fill.stretched.unwrap().rate - 0.8).abs() < 1e-9);
        assert!(fill.reached_back > 0.0);
        assert!((answer.pieces[1].start - 1.345).abs() < 1e-9);
    }
}
