//! Shared Crescendum album benchmark CLI (`rhythm-bench`).

mod beat_grid;
mod metrics;
mod rpp;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::{Parser, ValueEnum};
use rhythm_hits::{DetectorInput, DetectorOutput, DetectorRegistry, PieceInput};
use serde::{Deserialize, Serialize};

use crate::metrics::{
    ClassMetrics, SongGroundTruth, aggregate_metrics, extract_ground_truth, grouped_truth_and_preds,
    score_class,
};
use crate::rpp::{RenderedKit, parse_comped_rpp, render_comped_kit};

const DEFAULT_CORPUS: &str =
    "/run/media/AudioHaven/Project/Crescendum-Rockstars-SESSIONS-2026-10-08/Crescendum";

#[derive(Clone, Copy, Debug, ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
enum BenchMode {
    /// Pass acoustic close mics + extra mics (no triggers). Fair acoustic benchmark.
    MicOnly,
    /// Pass acoustic close mics + hardware piezo triggers (upper-bound fusion mode).
    WithTriggers,
    /// Pass only the mono summed acoustic drum_bus (for stereo/bus neural models).
    BusOnly,
    /// Pass only the close kick mic and score against the kick trigger.
    KickOnly,
    /// Pass only the close snare mic and score against the snare trigger.
    SnareOnly,
    /// Pass only the named tom close mic and score against its trigger.
    T1Only,
    T2Only,
    T3Only,
    T4Only,
}

impl BenchMode {
    fn solo_piece(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::KickOnly => Some(("kick", "kick_in")),
            Self::SnareOnly => Some(("snare", "snare_top")),
            Self::T1Only => Some(("t1", "t1")),
            Self::T2Only => Some(("t2", "t2")),
            Self::T3Only => Some(("t3", "t3")),
            Self::T4Only => Some(("t4", "t4")),
            _ => None,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "rhythm-bench", about = "Crescendum album drum hit benchmark harness")]
struct Cli {
    /// Path to reunited Crescendum folder containing `<song>/<song>.comped.RPP`.
    #[arg(long, default_value = DEFAULT_CORPUS)]
    corpus: PathBuf,

    /// Optional case-insensitive substring filter for song names (e.g. "set in stone").
    #[arg(long)]
    song: Option<String>,

    /// Registered detector ID from DetectorRegistry (ignored when --predictions is set).
    #[arg(long, default_value = "fts.hits.kit-fusion")]
    detector: String,

    /// Input mode passed to the detector.
    #[arg(long, value_enum, default_value_t = BenchMode::MicOnly)]
    mode: BenchMode,

    /// Optional external predictions JSON file (from Python/uv/ONNX scripts) to score
    /// against the exact same Crescendum ground-truth triggers.
    #[arg(long)]
    predictions: Option<PathBuf>,

    /// Export active-comp rendered WAV stems + `gt_onsets.json` into this directory and exit.
    #[arg(long)]
    export_stems: Option<PathBuf>,

    /// Optional output path to write canonical benchmark JSON results.
    #[arg(long)]
    out: Option<PathBuf>,

    /// Score beat and downbeat events against the REAPER project grid.
    #[arg(long, conflicts_with_all = ["predictions", "export_stems"])]
    beat_grid: bool,

    #[arg(long)]
    threshold_db: Option<f64>,

    #[arg(long, default_value_t = 10.0)]
    crest_db: f64,

    #[arg(long, default_value_t = 30.0)]
    retrig_ms: f64,
}

#[derive(Clone, Debug, Deserialize)]
struct ExternalHit {
    piece: String,
    at: f64,
}

#[derive(Clone, Debug, Deserialize)]
struct ExternalPredictions {
    engine: String,
    #[serde(default)]
    cpu_rtf: Option<f64>,
    songs: HashMap<String, Vec<ExternalHit>>,
}

#[derive(Clone, Debug, Serialize)]
struct SongBenchmark {
    song: String,
    bpm: f64,
    duration_secs: f64,
    elapsed_secs: f64,
    cpu_rtf: f64,
    dead_triggers: Vec<String>,
    by_class: BTreeMap<String, ClassMetrics>,
    overall: Option<ClassMetrics>,
}

#[derive(Clone, Debug, Serialize)]
struct CorpusBenchmark {
    engine: String,
    mode: String,
    corpus: String,
    song_count: usize,
    total_audio_secs: f64,
    total_elapsed_secs: f64,
    cpu_rtf: f64,
    by_class: BTreeMap<String, ClassMetrics>,
    overall: ClassMetrics,
    songs: Vec<SongBenchmark>,
}

fn discover_comped_rpps(corpus: &Path, filter: Option<&str>) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    let entries = std::fs::read_dir(corpus)
        .map_err(|e| format!("read corpus dir {}: {e}", corpus.display()))?;
    let filter_low = filter.map(str::to_ascii_lowercase);

    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        if let Ok(sub) = std::fs::read_dir(&dir) {
            for f in sub.flatten() {
                let p = f.path();
                if let Some(name) = p.file_name().and_then(|s| s.to_str())
                    && name.ends_with(".comped.RPP")
                {
                    if let Some(ref flt) = filter_low
                        && !name.to_ascii_lowercase().contains(flt)
                    {
                        continue;
                    }
                    found.push(p);
                }
            }
        }
    }
    found.sort();
    Ok(found)
}

fn write_wav_mono(path: &Path, samples: &[f32], sr: f64) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sr.round() as u32,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec)
        .map_err(|e| format!("create {}: {e}", path.display()))?;
    for &s in samples {
        w.write_sample(s)
            .map_err(|e| format!("write {}: {e}", path.display()))?;
    }
    w.finalize()
        .map_err(|e| format!("finalize {}: {e}", path.display()))
}

fn export_song_stems(out_root: &Path, kit: &RenderedKit, gt: &SongGroundTruth) -> Result<(), String> {
    let dir = out_root.join(&kit.song_name);
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    for key in [
        "drum_bus",
        "kick_in",
        "snare_top",
        "t1",
        "t2",
        "t3",
        "t4",
        "ohs",
        "hihat",
        "ride",
        "rooms",
    ] {
        if let Some(buf) = kit.tracks.get(key) {
            write_wav_mono(&dir.join(format!("{key}.wav")), buf, kit.sample_rate)?;
        }
    }
    let gt_json = serde_json::to_string_pretty(gt).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("gt_onsets.json"), gt_json)
        .map_err(|e| format!("write gt_onsets.json: {e}"))?;
    Ok(())
}

fn run_rust_detector(
    registry: &DetectorRegistry,
    detector_id: &str,
    mode: BenchMode,
    kit: &RenderedKit,
    cli: &Cli,
) -> Result<(DetectorOutput, f64), String> {
    let detector = registry
        .get(detector_id)
        .ok_or_else(|| format!("unknown detector '{detector_id}'"))?;

    let t0 = Instant::now();
    let out = match mode {
        BenchMode::BusOnly
        | BenchMode::KickOnly
        | BenchMode::SnareOnly
        | BenchMode::T1Only
        | BenchMode::T2Only
        | BenchMode::T3Only
        | BenchMode::T4Only => {
            let (name, track) = mode.solo_piece().unwrap_or(("drum_bus", "drum_bus"));
            let mic = kit
                .tracks
                .get(track)
                .ok_or_else(|| format!("missing {track} in {}", kit.song_name))?;
            let pieces = [PieceInput {
                name: name.to_owned(),
                mic: mic.as_slice(),
                trigger: None,
            }];
            detector.try_detect(&DetectorInput {
                sample_rate: kit.sample_rate,
                pieces: &pieces,
                extra_mics: &[],
                threshold_db: cli.threshold_db,
                crest_db: cli.crest_db,
                retrig_ms: cli.retrig_ms,
                tempo_bpm: Some(kit.bpm),
                time_sig: Some(kit.time_sig),
            })?
        }
        BenchMode::MicOnly | BenchMode::WithTriggers => {
            let use_trig = matches!(mode, BenchMode::WithTriggers);
            let spec = [
                ("kick", "kick_in", "kick_trig"),
                ("snare", "snare_top", "snare_trig"),
                ("t1", "t1", "t1_trig"),
                ("t2", "t2", "t2_trig"),
                ("t3", "t3", "t3_trig"),
                ("t4", "t4", "t4_trig"),
            ];
            let mut pieces = Vec::new();
            for (name, mic_k, trig_k) in spec {
                if let Some(mic) = kit.tracks.get(mic_k) {
                    let trigger = if use_trig {
                        kit.tracks.get(trig_k).map(Vec::as_slice)
                    } else {
                        None
                    };
                    pieces.push(PieceInput {
                        name: name.to_owned(),
                        mic: mic.as_slice(),
                        trigger,
                    });
                }
            }
            if detector_id.starts_with("fts.hits.adtof-onnx") {
                for name in ["drum_bus", "ohs"] {
                    if let Some(mic) = kit.tracks.get(name) {
                        pieces.push(PieceInput {
                            name: name.to_owned(),
                            mic,
                            trigger: None,
                        });
                    }
                }
            }
            let mut extras = Vec::new();
            for k in ["ohs", "hihat", "ride", "rooms"] {
                if let Some(m) = kit.tracks.get(k) {
                    extras.push(m.as_slice());
                }
            }
            detector.try_detect(&DetectorInput {
                sample_rate: kit.sample_rate,
                pieces: &pieces,
                extra_mics: &extras,
                threshold_db: cli.threshold_db,
                crest_db: cli.crest_db,
                retrig_ms: cli.retrig_ms,
                tempo_bpm: Some(kit.bpm),
                time_sig: Some(kit.time_sig),
            })?
        }
    };
    let elapsed = t0.elapsed().as_secs_f64();
    Ok((out, elapsed))
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    let rpps = discover_comped_rpps(&cli.corpus, cli.song.as_deref())?;
    if rpps.is_empty() {
        return Err(format!("no .comped.RPP files found under {}", cli.corpus.display()));
    }

    let external: Option<ExternalPredictions> = match &cli.predictions {
        Some(p) => {
            let text = std::fs::read_to_string(p)
                .map_err(|e| format!("read predictions {}: {e}", p.display()))?;
            Some(serde_json::from_str(&text).map_err(|e| format!("parse predictions: {e}"))?)
        }
        None => None,
    };

    let solo_piece = cli.mode.solo_piece().map(|(piece, _)| piece);
    if solo_piece.is_some() && (cli.beat_grid || cli.export_stems.is_some()) {
        return Err("Individual close-mic modes require a drum hit benchmark".into());
    }
    let registry = DetectorRegistry::default();
    if cli.beat_grid {
        return beat_grid::run(&cli, &registry, &rpps);
    }
    let mut song_results = Vec::new();
    let mut all_class_metrics: BTreeMap<String, Vec<ClassMetrics>> = BTreeMap::new();
    let mut all_overall_metrics: Vec<ClassMetrics> = Vec::new();
    let mut total_audio_secs = 0.0_f64;
    let mut total_elapsed_secs = 0.0_f64;

    for rpp_path in &rpps {
        let session = parse_comped_rpp(rpp_path)?;
        let kit = render_comped_kit(&session)?;
        let mut gt = extract_ground_truth(&kit);
        if let Some(piece) = solo_piece {
            gt.pieces.retain(|p| p.piece == piece);
        }

        if let Some(ref export_dir) = cli.export_stems {
            export_song_stems(export_dir, &kit, &gt)?;
            println!(
                "exported stems: {} ({:.1}s @ {:.0} Hz, {} healthy triggers)",
                kit.song_name,
                kit.duration_secs,
                kit.sample_rate,
                gt.pieces.iter().filter(|p| p.healthy).count()
            );
            continue;
        }

        let (preds, elapsed): (Vec<_>, _) = if let Some(ref ext) = external {
            let list = ext
                .songs
                .get(&kit.song_name)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|h| (h.piece, h.at))
                .collect();
            let el = ext.cpu_rtf.map_or(0.0, |rtf| rtf * kit.duration_secs);
            (list, el)
        } else {
            let (output, elapsed) =
                run_rust_detector(&registry, &cli.detector, cli.mode, &kit, &cli)?;
            let hits = output.hits.into_iter().map(|h| (h.piece, h.at)).collect();
            (hits, elapsed)
        };

        let grouped = grouped_truth_and_preds(&gt, &preds);
        let mut by_class = BTreeMap::new();
        let mut song_class_list = Vec::new();
        let mut song_errs = Vec::new();

        for cls in ["kick", "snare", "toms"] {
            if let Some((truth, pred)) = grouped.get(cls) {
                let m = score_class(truth, pred);
                if m.tp_50ms > 0 {
                    song_errs.push(m.median_abs_err_ms);
                }
                all_class_metrics
                    .entry(cls.to_owned())
                    .or_default()
                    .push(m.clone());
                song_class_list.push(m.clone());
                by_class.insert(cls.to_owned(), m);
            }
        }
        let overall = (solo_piece.is_none() || !song_class_list.is_empty())
            .then(|| aggregate_metrics(&song_class_list, &song_errs));
        if let Some(score) = &overall {
            all_overall_metrics.push(score.clone());
        }

        let dead_triggers: Vec<String> = gt
            .pieces
            .iter()
            .filter(|p| !p.healthy)
            .map(|p| format!("{} ({})", p.piece, p.health_note))
            .collect();

        let rtf = if kit.duration_secs > 0.0 {
            elapsed / kit.duration_secs
        } else {
            0.0
        };
        total_audio_secs += kit.duration_secs;
        total_elapsed_secs += elapsed;

        if let Some(overall) = &overall {
            println!(
                "{:<22} | {:3.0} BPM | F1@20ms={:.3} (P={:.3} R={:.3}) | F1@50ms={:.3} | med_err={:4.2}ms | RTF={:.4}x | dead={}",
                kit.song_name,
                kit.bpm,
                overall.f1_20ms,
                overall.precision_20ms,
                overall.recall_20ms,
                overall.f1_50ms,
                overall.median_abs_err_ms,
                rtf,
                dead_triggers.len()
            );
        } else {
            println!(
                "{:<22} | {:3.0} BPM | unscored (no healthy {} trigger) | RTF={:.4}x",
                kit.song_name,
                kit.bpm,
                solo_piece.unwrap_or("selected piece"),
                rtf,
            );
        }

        song_results.push(SongBenchmark {
            song: kit.song_name,
            bpm: kit.bpm,
            duration_secs: kit.duration_secs,
            elapsed_secs: elapsed,
            cpu_rtf: rtf,
            dead_triggers,
            by_class,
            overall,
        });
    }

    if cli.export_stems.is_some() {
        return Ok(());
    }

    if solo_piece.is_some() && all_overall_metrics.is_empty() {
        return Err("none of the selected songs has a healthy trigger for this piece".into());
    }
    let mut agg_by_class = BTreeMap::new();
    for (cls, list) in &all_class_metrics {
        let errs: Vec<f64> = list
            .iter()
            .filter(|m| m.tp_50ms > 0)
            .map(|m| m.median_abs_err_ms)
            .collect();
        agg_by_class.insert(cls.clone(), aggregate_metrics(list, &errs));
    }
    let overall_errs: Vec<f64> = all_overall_metrics
        .iter()
        .filter(|m| m.tp_50ms > 0)
        .map(|m| m.median_abs_err_ms)
        .collect();
    let overall = aggregate_metrics(&all_overall_metrics, &overall_errs);
    let cpu_rtf = if total_audio_secs > 0.0 {
        total_elapsed_secs / total_audio_secs
    } else {
        0.0
    };

    let engine = external
        .as_ref()
        .map_or_else(|| cli.detector.clone(), |e| e.engine.clone());
    let mode_str = if let Some(piece) = solo_piece {
        format!("{piece}-only")
    } else if external.is_some() {
        "external-predictions".to_owned()
    } else {
        format!("{:?}", cli.mode).to_ascii_lowercase()
    };

    println!("
=== SUMMARY ({engine}, mode={mode_str}, {} songs, {:.1}s audio) ===", song_results.len(), total_audio_secs);
    for (cls, m) in &agg_by_class {
        println!(
            "  {:<8}: F1@20ms={:.3} (P={:.3} R={:.3}) | F1@50ms={:.3} | med_err={:.2}ms | truth={} pred={}",
            cls, m.f1_20ms, m.precision_20ms, m.recall_20ms, m.f1_50ms, m.median_abs_err_ms, m.truth_count, m.pred_count
        );
    }
    println!(
        "  {:<8}: F1@20ms={:.3} (P={:.3} R={:.3}) | F1@50ms={:.3} | med_err={:.2}ms | CPU RTF={:.4}x",
        "OVERALL", overall.f1_20ms, overall.precision_20ms, overall.recall_20ms, overall.f1_50ms, overall.median_abs_err_ms, cpu_rtf
    );

    let report = CorpusBenchmark {
        engine,
        mode: mode_str,
        corpus: cli.corpus.display().to_string(),
        song_count: song_results.len(),
        total_audio_secs,
        total_elapsed_secs,
        cpu_rtf,
        by_class: agg_by_class,
        overall,
        songs: song_results,
    };

    if let Some(ref out_path) = cli.out {
        if let Some(parent) = out_path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
        std::fs::write(out_path, json)
            .map_err(|e| format!("write {}: {e}", out_path.display()))?;
        println!("wrote benchmark JSON: {}", out_path.display());
    }

    Ok(())
}
