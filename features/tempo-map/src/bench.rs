//! The benchmark: every song of a test set, mapped and compared with the
//! map the owner accepted for it, so a change to an algorithm shows as a
//! number.
//!
//! A test set is a folder of songs, each a folder with the song's stems
//! and its accepted project (`*Tempo Mapped*.RPP`). A second stem set of
//! the same songs (another separator's) and the old scripts' beats can be
//! measured beside it: a map must come out the same from either set, and
//! the scripts' numbers are the bar to clear.

use std::path::{Path, PathBuf};

use crate::analysis::{self, Analysis, Options};
use crate::eval::{self, Comparison, Tracked};
use crate::features::{self, Features};
use crate::map::{MapOptions, TempoMap};
use crate::meter::Layout;
use crate::rpp;
use crate::track::TrackParams;

/// A bar differs from the reference past this, ms.
pub const TOLERANCE_MS: f64 = 25.0;

/// One song of the test set.
#[derive(Debug, Clone)]
pub struct Case {
    pub slug: String,
    pub stems: PathBuf,
    pub reference: PathBuf,
}

/// The songs of `set`: each folder holding an accepted project.
#[must_use]
pub fn cases(set: &Path) -> Vec<Case> {
    let Ok(entries) = std::fs::read_dir(set) else {
        return Vec::new();
    };
    let mut out: Vec<Case> = entries
        .flatten()
        .filter_map(|entry| {
            let dir = entry.path();
            let reference = std::fs::read_dir(&dir)
                .ok()?
                .flatten()
                .map(|e| e.path())
                .find(|p| {
                    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("rpp"))
                        && p.file_name()
                            .is_some_and(|n| n.to_string_lossy().contains("Tempo Mapped"))
                })?;
            Some(Case {
                slug: dir.file_name()?.to_string_lossy().into_owned(),
                stems: dir,
                reference,
            })
        })
        .collect();
    out.sort_by(|a, b| a.slug.cmp(&b.slug));
    out
}

/// One way of making a map, measured against the reference.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Row {
    pub song: String,
    /// The stem set: `set` (the songs' own) or `alt`.
    pub stems: String,
    /// `scripts` (the old scripts' beats), `port` (the same steps here)
    /// or `studio` (what an analysis does: the beats before the first hit
    /// keep their spacing, added and dropped beats are repaired).
    pub method: String,
    pub beats: usize,
    /// How far the nearest beat is from the reference's bar 1, ms.
    pub bar1_ms: f64,
    /// Where the best candidate for bar 1 is, in beats from the
    /// reference's (0: right). None for the scripts, which do not guess.
    pub bar1_guess: Option<i64>,
    /// The place of the reference's bar 1 among the candidates, from 1.
    pub bar1_rank: Option<usize>,
    /// The downbeats against the reference's.
    pub bars: usize,
    pub median_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
    pub differing: usize,
    /// The beats added or dropped against the reference.
    pub slips: usize,
    /// Where: each bar from which the map is a whole number of beats off
    /// the reference, and by how many.
    pub slipped_at: Vec<eval::BeatSlip>,
    /// The phase line from bar 1.
    pub phase_line: String,
    /// The beats on hits against the per-bar grid, median ms.
    pub grid_ms: f64,
    /// The share of grid beats with a kit hit within 35 ms.
    pub on_hits: f64,
}

/// One method's maps from the two stem sets, against each other.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Agreement {
    pub song: String,
    pub method: String,
    pub bars: usize,
    pub median_ms: f64,
    pub max_ms: f64,
    pub differing: usize,
    pub slips: usize,
}

/// The benchmark's result.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Report {
    pub rows: Vec<Row>,
    pub agreement: Vec<Agreement>,
    /// What could not be run, and why.
    pub skipped: Vec<String>,
}

/// The beats a script run saved (`<slug>_s.json`: `beats` and `src`).
fn script_beats(path: &Path) -> Option<(Vec<f64>, Vec<String>)> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let beats: Vec<f64> = value
        .get("beats")?
        .as_array()?
        .iter()
        .filter_map(serde_json::Value::as_f64)
        .collect();
    let sources: Vec<String> = value.get("src").and_then(|s| s.as_array()).map_or_else(
        || vec!["hit".to_owned(); beats.len()],
        |a| {
            a.iter()
                .map(|s| s.as_str().unwrap_or("hit").to_owned())
                .collect()
        },
    );
    Some((beats, sources))
}

/// The beat of `beats` nearest `time`, and how far it is, ms.
fn nearest(beats: &[f64], time: f64) -> (usize, f64) {
    let i = (0..beats.len())
        .min_by(|a, b| {
            (beats[*a] - time)
                .abs()
                .total_cmp(&(beats[*b] - time).abs())
        })
        .unwrap_or(0);
    (i, beats.get(i).map_or(0.0, |b| (b - time) * 1000.0))
}

/// The map of `beats` with bar 1 where the reference has it, and its row.
#[allow(clippy::too_many_arguments)]
fn measure(
    song: &str,
    stems: &str,
    method: &str,
    beats: &[f64],
    sources: &[String],
    analysis: Option<&Analysis>,
    reference: &TempoMap,
    features: &Features,
) -> (Row, TempoMap) {
    let start = reference.bars.first().map_or(0.0, |b| b.time);
    let (bar1, bar1_ms) = nearest(beats, start);
    let layout = analysis.map_or_else(Layout::default, |a| a.layout.clone());
    let map = TempoMap::build(
        beats,
        sources,
        &MapOptions {
            bar1,
            layout,
            raw_from_bar: None,
        },
    );
    let diff: Comparison = eval::compare(reference, &map, TOLERANCE_MS);
    let measured = eval::evaluate(
        &map,
        features,
        Some(Tracked {
            beats,
            sources,
            bar1,
        }),
    );
    let row = Row {
        song: song.to_owned(),
        stems: stems.to_owned(),
        method: method.to_owned(),
        beats: beats.len(),
        bar1_ms,
        bar1_guess: analysis
            .and_then(|a| a.candidates.first())
            .map(|c| c.beat as i64 - bar1 as i64),
        bar1_rank: analysis
            .and_then(|a| a.candidates.iter().position(|c| c.beat == bar1))
            .map(|p| p + 1),
        bars: diff.bars,
        median_ms: diff.downbeat.median,
        p95_ms: diff.downbeat.p95,
        max_ms: diff.downbeat.max,
        differing: diff.differing,
        slips: diff.slips.len(),
        slipped_at: diff.slips.clone(),
        phase_line: measured.phase_line.clone(),
        grid_ms: measured.grid_error.map_or(0.0, |g| g.median),
        on_hits: measured.on_hits,
    };
    (row, map)
}

/// Runs the benchmark over `set`; with `alt`, the same songs' other stem
/// set; with `scripts`, a folder of the old scripts' beats
/// (`<scripts>/set/<slug>_s.json`, `<scripts>/alt/<slug>_s.json`).
/// `progress` hears each song as it starts.
#[must_use]
pub fn run(
    set: &Path,
    alt: Option<&Path>,
    scripts: Option<&Path>,
    mut progress: impl FnMut(&str),
) -> Report {
    let mut report = Report::default();
    for case in cases(set) {
        let reference = match rpp::reference(&case.reference) {
            Ok(map) => map,
            Err(e) => {
                report.skipped.push(e);
                continue;
            }
        };
        let bpm = reference.bars.first().map_or(120.0, |b| b.bpm).round();
        let mut maps: Vec<(String, String, TempoMap)> = Vec::new();
        let folders = [
            ("set", Some(case.stems.clone())),
            ("alt", alt.map(|a| a.join(&case.slug))),
        ];
        for (name, folder) in folders {
            let Some(folder) = folder.filter(|f| f.is_dir()) else {
                continue;
            };
            progress(&format!("{} ({name})", case.slug));
            let features = match features::extract(&folder) {
                Ok(f) => f,
                Err(e) => {
                    report.skipped.push(format!("{} ({name}): {e}", case.slug));
                    continue;
                }
            };
            if let Some((beats, sources)) = scripts
                .map(|s| s.join(name).join(format!("{}_s.json", case.slug)))
                .and_then(|p| script_beats(&p))
            {
                let (row, map) = measure(
                    &case.slug, name, "scripts", &beats, &sources, None, &reference, &features,
                );
                report.rows.push(row);
                maps.push((name.to_owned(), "scripts".to_owned(), map));
            }
            for (method, no_repair) in [("port", true), ("studio", false)] {
                let options = Options {
                    track: TrackParams {
                        bpm,
                        // The port is the scripts step for step.
                        stack_edges: no_repair,
                        ..TrackParams::default()
                    },
                    meter: None,
                    changes: Vec::new(),
                    no_repair,
                };
                let found = analysis::analyse(&features, &options);
                let (row, map) = measure(
                    &case.slug,
                    name,
                    method,
                    &found.beats,
                    &found.beat_sources,
                    Some(&found),
                    &reference,
                    &features,
                );
                report.rows.push(row);
                maps.push((name.to_owned(), method.to_owned(), map));
            }
        }
        for method in ["scripts", "port", "studio"] {
            let of = |stems: &str| {
                maps.iter()
                    .find(|(s, m, _)| s == stems && m == method)
                    .map(|(_, _, map)| map)
            };
            if let (Some(a), Some(b)) = (of("set"), of("alt")) {
                let diff = eval::compare(a, b, TOLERANCE_MS);
                report.agreement.push(Agreement {
                    song: case.slug.clone(),
                    method: method.to_owned(),
                    bars: diff.bars,
                    median_ms: diff.downbeat.median,
                    max_ms: diff.downbeat.max,
                    differing: diff.differing,
                    slips: diff.slips.len(),
                });
            }
        }
    }
    report
}

impl Report {
    /// The report as Markdown tables.
    #[must_use]
    pub fn markdown(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "| song | stems | method | beats | bars | median ms | p95 ms | max ms | bars > {TOLERANCE_MS:.0} ms | ±beats | grid ms | on hits | bar 1 guess | rank |"
        );
        let _ = writeln!(
            out,
            "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
        );
        for r in &self.rows {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} | {:.1} | {:.1} | {:.1} | {} | {} | {:.1} | {:.0}% | {} | {} |",
                r.song,
                r.stems,
                r.method,
                r.beats,
                r.bars,
                r.median_ms,
                r.p95_ms,
                r.max_ms,
                r.differing,
                r.slips,
                r.grid_ms,
                r.on_hits * 100.0,
                r.bar1_guess
                    .map_or_else(|| "–".to_owned(), |g| format!("{g:+}")),
                r.bar1_rank
                    .map_or_else(|| "–".to_owned(), |g| g.to_string()),
            );
        }
        if !self.agreement.is_empty() {
            let _ = writeln!(
                out,
                "\n| song | method | bars | set vs alt median ms | max ms | bars > {TOLERANCE_MS:.0} ms | ±beats |"
            );
            let _ = writeln!(out, "|---|---|---|---|---|---|---|");
            for a in &self.agreement {
                let _ = writeln!(
                    out,
                    "| {} | {} | {} | {:.1} | {:.1} | {} | {} |",
                    a.song, a.method, a.bars, a.median_ms, a.max_ms, a.differing, a.slips
                );
            }
        }
        let _ = writeln!(
            out,
            "\nWhere a map is whole beats off its reference (bar: beats):\n"
        );
        for r in self.rows.iter().filter(|r| !r.slipped_at.is_empty()) {
            let list: Vec<String> = r
                .slipped_at
                .iter()
                .map(|s| format!("{}: {:+}", s.bar, s.beats))
                .collect();
            let _ = writeln!(
                out,
                "- {} ({}, {}): {}",
                r.song,
                r.stems,
                r.method,
                list.join(", ")
            );
        }
        for s in &self.skipped {
            let _ = writeln!(out, "\nskipped: {s}");
        }
        out
    }
}
