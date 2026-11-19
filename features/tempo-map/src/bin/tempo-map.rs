//! `tempo-map`: the tempo-map library from a shell.
//!
//! ```text
//! tempo-map analyse <stems folder> --bpm 107 [-o song.json]
//! tempo-map candidates song.json
//! tempo-map map song.json [--bar1 N]
//! tempo-map evaluate <stems folder> song.json|project.RPP
//! tempo-map compare a.json|a.RPP b.json|b.RPP
//! tempo-map hits song.json
//! tempo-map bench <songs folder> [--alt <folder>] [--scripts <folder>]
//! ```
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command line prints"
)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tempo_map::analysis::{self, Analysis, Options};
use tempo_map::map::TempoMap;
use tempo_map::meter::{Meter, MeterChange};
use tempo_map::track::TrackParams;
use tempo_map::{bench, eval, features, rpp};

#[derive(Parser)]
#[command(name = "tempo-map", about = "Tempo maps from a recording's stems")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Analyse a song from its stems: beats, hits, the phase check, the
    /// candidates for bar 1.
    Analyse {
        /// The folder of `<song> (<Name>).wav` stems.
        stems: PathBuf,
        /// The song's tempo, roughly (it decides half or double).
        #[arg(long, default_value_t = 120.0)]
        bpm: f64,
        /// The meter (`3/4`); what the beats say when left out.
        #[arg(long)]
        meter: Option<String>,
        /// Meter changes, `bar:meter` (`17:2/4`).
        #[arg(long = "change")]
        changes: Vec<String>,
        #[arg(long, default_value_t = 400.0)]
        tightness: f64,
        /// Drop beats before this, seconds.
        #[arg(long, default_value_t = 0.0)]
        from: f64,
        /// Leave added and dropped beats as tracked.
        #[arg(long)]
        no_repair: bool,
        /// Where to save the analysis.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// The candidates for bar 1 of a saved analysis.
    Candidates { analysis: PathBuf },
    /// The tempo map of a saved analysis: one point per bar.
    Map {
        analysis: PathBuf,
        /// The beat that is bar 1's downbeat.
        #[arg(long)]
        bar1: Option<usize>,
        /// Take the downbeats as played from this song bar (a ritard).
        #[arg(long)]
        raw_from_bar: Option<u32>,
    },
    /// Measure a map (an analysis or a REAPER project) against stems.
    Evaluate {
        stems: PathBuf,
        map: PathBuf,
        /// Every bar, not only the totals.
        #[arg(long)]
        bars: bool,
    },
    /// Two maps (analyses or REAPER projects), bar by bar.
    Compare {
        a: PathBuf,
        b: PathBuf,
        #[arg(long, default_value_t = bench::TOLERANCE_MS)]
        tolerance_ms: f64,
        #[arg(long)]
        bars: bool,
    },
    /// Every kit piece's hits with their bars and beats.
    Hits { analysis: PathBuf },
    /// Run every song of a test set against its accepted map.
    Bench {
        /// The songs: a folder per song with its stems and `*Tempo Mapped*.RPP`.
        songs: PathBuf,
        /// The same songs' stems from another separator.
        #[arg(long)]
        alt: Option<PathBuf>,
        /// The old scripts' beats: `set/<slug>_s.json`, `alt/<slug>_s.json`.
        #[arg(long)]
        scripts: Option<PathBuf>,
        /// Where to save the report as JSON.
        #[arg(long)]
        json: Option<PathBuf>,
    },
}

fn load_analysis(path: &Path) -> Result<Analysis, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// A map from a REAPER project, a saved analysis or a saved map.
fn load_map(path: &Path) -> Result<TempoMap, String> {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("rpp"))
    {
        return rpp::reference(path);
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Ok(found) = serde_json::from_str::<Analysis>(&text) {
        return Ok(found.map());
    }
    serde_json::from_str::<TempoMap>(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn json<T: serde::Serialize>(value: &T) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Analyse {
            stems,
            bpm,
            meter,
            changes,
            tightness,
            from,
            no_repair,
            out,
        } => {
            let meter = meter
                .map(|m| Meter::parse(&m).ok_or(format!("{m} is not a meter")))
                .transpose()?;
            let changes = changes
                .iter()
                .map(|c| {
                    let (bar, meter) = c.split_once(':').ok_or(format!("{c}: bar:meter"))?;
                    Ok(MeterChange {
                        bar: bar.parse().map_err(|_| format!("{bar} is not a bar"))?,
                        meter: Meter::parse(meter).ok_or(format!("{meter} is not a meter"))?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let measured = features::extract(&stems).map_err(|e| e.to_string())?;
            let found = analysis::analyse(
                &measured,
                &Options {
                    track: TrackParams {
                        bpm,
                        tightness,
                        from,
                        ..TrackParams::default()
                    },
                    meter,
                    changes,
                    no_repair,
                },
            );
            if let Some(out) = out {
                let text = serde_json::to_string(&found).map_err(|e| e.to_string())?;
                std::fs::write(&out, text).map_err(|e| format!("{}: {e}", out.display()))?;
            }
            let line = found.phase_line(&measured);
            json(&serde_json::json!({
                "summary": found.summary(line),
                "meter_guess": found.meter_guess,
                "repair": found.repair,
                "candidates": found.candidates.iter().take(4).collect::<Vec<_>>(),
                "evaluation": {
                    "grid_error": found.evaluate(&measured).grid_error,
                    "beat_error": found.evaluate(&measured).beat_error,
                },
            }))
        }
        Command::Candidates { analysis } => json(&load_analysis(&analysis)?.candidates),
        Command::Map {
            analysis,
            bar1,
            raw_from_bar,
        } => {
            let mut found = load_analysis(&analysis)?;
            if let Some(beat) = bar1 {
                found.set_bar1(beat)?;
            }
            if raw_from_bar.is_some() {
                found.set_raw_from(raw_from_bar);
            }
            let map = found.map();
            json(&serde_json::json!({ "map": map, "points": map.points() }))
        }
        Command::Evaluate { stems, map, bars } => {
            let measured = features::extract(&stems).map_err(|e| e.to_string())?;
            let mut result = match load_analysis(&map) {
                Ok(found) => found.evaluate(&measured),
                Err(_) => eval::evaluate(&load_map(&map)?, &measured, None),
            };
            if !bars {
                result.per_bar.clear();
            }
            json(&result)
        }
        Command::Compare {
            a,
            b,
            tolerance_ms,
            bars,
        } => {
            let mut diff = eval::compare(&load_map(&a)?, &load_map(&b)?, tolerance_ms);
            if !bars {
                diff.per_bar.retain(|d| d.ms.abs() > tolerance_ms);
            }
            json(&diff)
        }
        Command::Hits { analysis } => json(&load_analysis(&analysis)?.hit_marks()),
        Command::Bench {
            songs,
            alt,
            scripts,
            json: out,
        } => {
            let report = bench::run(&songs, alt.as_deref(), scripts.as_deref(), |song| {
                eprintln!("{song}");
            });
            if report.rows.is_empty() {
                return Err(format!(
                    "{}: no song with stems and a `*Tempo Mapped*.RPP`",
                    songs.display()
                ));
            }
            if let Some(out) = out {
                let text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
                std::fs::write(&out, text).map_err(|e| format!("{}: {e}", out.display()))?;
            }
            println!("{}", report.markdown());
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tempo-map: {e}");
            ExitCode::FAILURE
        }
    }
}
