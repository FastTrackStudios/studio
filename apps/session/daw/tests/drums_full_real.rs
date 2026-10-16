//! The whole drum edit of a real song, timed end to end: the kit comped
//! from its best takes phrase by phrase, quantized over the whole song,
//! then a hand's worth of hits dragged — on a copy of the song, through the
//! Action List as a model drives it over MCP. Each step's time is reported,
//! and the edit (comp + quantize + drags) is held to `FTS_BENCH_BUDGET_MS`
//! when it is set (the goal: 2000).
//!
//! Hit detection is a step of its own, run only with `FTS_BENCH_DETECT=1`
//! (a song's takes are analysed ahead of the edit).
//!
//! ```sh
//! just drums-bench /path/to/Unbreakable.session        # a fresh copy, run, report
//! ```
//!
//! or by hand, on a copy (with its `-wal`):
//!
//! ```sh
//! FTS_PERF_SESSION=/path/to/copy.session [FTS_BENCH_OUT=report.json] [FTS_BENCH_BUDGET_MS=2000] \
//!   cargo nextest run --cargo-profile release-fast -p session-daw --features signal \
//!   --test drums_full_real --run-ignored all
//! ```

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use session_daw::actions::{list, take_window, window_pending};
use session_daw::arrange_edit::{Editor, Effect, SlipCut, kit_items, seconds_of};
use session_daw::keys::Action;
use session_daw::studio::StudioSession;

/// How many bars a comp stretch spans: a phrase.
const PHRASE_BARS: f64 = 4.0;

/// How many hits a hand drags after the quantize.
const DRAGS: usize = 20;

/// How far each is dragged, ms.
const DRAG_MS: f64 = 6.0;

/// Be the window for what the Action List queued for it: a slip the
/// quantize hands it, a restore a comp hands it.
fn be_the_window(path: &Path) {
    let seconds = |us: i64| num_traits::ToPrimitive::to_f64(&us).unwrap_or(0.0) / 1_000_000.0;
    for action in take_window() {
        let session = StudioSession::read_current(Some(path), None).expect("reads back");
        let mut project = (*session.project.0).clone();
        let mut effects = Vec::new();
        let ticket = match action {
            Action::SlipDrums {
                moves,
                pre_roll_us,
                crossfade_us,
                stretch_over_us,
                attacks_us,
                ticket,
            } => {
                let items = kit_items(&project, &[]);
                let cut = SlipCut {
                    pre_roll: seconds(pre_roll_us),
                    crossfade: seconds(crossfade_us),
                    stretch_over: (stretch_over_us > 0).then(|| seconds(stretch_over_us)),
                    attacks: attacks_us.iter().map(|us| seconds(*us)).collect(),
                };
                Editor::slip_items(
                    &mut project,
                    &items,
                    &seconds_of(&moves),
                    ("Slip edit drums", cut),
                    &mut effects,
                );
                ticket
            }
            Action::RestoreDrums {
                from_us,
                to_us,
                ticket,
            } => {
                let folder = project
                    .tracks
                    .iter()
                    .find(|t| t.parent_guid.is_none() && t.name == "Drums")
                    .map(|t| t.guid.clone())
                    .expect("the kit");
                Editor::restore(
                    &mut project,
                    &folder,
                    seconds(from_us),
                    seconds(to_us),
                    &mut effects,
                );
                ticket
            }
            _ => continue,
        };
        for effect in effects {
            if let Effect::Send(edit) = effect {
                assert!(session_daw::engine::apply_now(&edit), "{edit:?}");
            }
        }
        assert!(session_daw::engine::apply_now(
            &session_daw::engine::Edit::Done(ticket)
        ));
    }
}

/// Run an action as MCP would, being the window for what it queues. What
/// it said, and how long the window's part took.
fn run(path: &Path, id: &'static str, args: serde_json::Value) -> (serde_json::Value, Duration) {
    let call = std::thread::spawn(move || list().run_json(id, args.to_string().as_bytes()));
    let began = Instant::now();
    let mut window = Duration::ZERO;
    while !call.is_finished() {
        assert!(began.elapsed() < Duration::from_secs(600), "{id} finishes");
        if window_pending() {
            let at = Instant::now();
            be_the_window(path);
            window += at.elapsed();
        }
        std::thread::sleep(Duration::from_micros(200));
    }
    let said = call
        .join()
        .expect("ran")
        .unwrap_or_else(|e| panic!("{id}: {e}"));
    (serde_json::from_slice(&said).unwrap_or_default(), window)
}

/// One step of the edit, timed.
#[derive(Default, Debug)]
struct Step {
    name: &'static str,
    ms: f64,
    /// Of which the window's part (a slip or restore it was handed).
    window_ms: f64,
    calls: usize,
    note: String,
}

impl Step {
    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "step": self.name, "ms": round(self.ms), "window_ms": round(self.window_ms),
            "calls": self.calls, "note": self.note,
        })
    }
}

fn round(ms: f64) -> f64 {
    (ms * 10.0).round() / 10.0
}

/// The song's phrases: each section cut into stretches of about
/// [`PHRASE_BARS`] bars.
fn phrases(path: &Path) -> Vec<(String, f64, f64)> {
    let (sections, _) = run(path, "SESSION_SECTIONS", serde_json::json!({}));
    let sections = sections
        .get("sections")
        .cloned()
        .unwrap_or(sections)
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for s in sections {
        let (Some(start), Some(end)) = (s["start"].as_f64(), s["end"].as_f64()) else {
            continue;
        };
        let bars = match (s["first_bar"].as_f64(), s["last_bar"].as_f64()) {
            (Some(a), Some(b)) => (b - a + 1.0).max(1.0),
            _ => 4.0,
        };
        let parts = (bars / PHRASE_BARS).ceil().max(1.0);
        let step = (end - start) / parts;
        let name = s["name"].as_str().unwrap_or("?").to_owned();
        let count = num_traits::ToPrimitive::to_usize(&parts).unwrap_or(1);
        for k in 0..count {
            let k = num_traits::ToPrimitive::to_f64(&k).unwrap_or(0.0);
            out.push((name.clone(), start + k * step, start + (k + 1.0) * step));
        }
    }
    out
}

/// The best judged take for a stretch, when it is not the one playing.
fn best_other(ranked: &serde_json::Value) -> Option<String> {
    let takes = ranked[0]["takes"].as_array()?;
    let best = takes
        .iter()
        .find(|t| t["undetected"] == false && t["empty"] == false)?;
    (best["current"] != true)
        .then(|| best["take"].as_str().map(str::to_owned))
        .flatten()
}

#[test]
#[ignore = "edits a real session; set FTS_PERF_SESSION to a copy"]
fn the_whole_drum_edit_of_a_real_song() {
    let Some(path) = std::env::var_os("FTS_PERF_SESSION").map(PathBuf::from) else {
        return;
    };
    {
        use tracing_subscriber::layer::SubscriberExt as _;
        let filter = architect_telemetry::timing::DIRECTIVES
            .iter()
            .fold(tracing_subscriber::EnvFilter::new("warn"), |f, d| {
                f.add_directive(d.parse().expect("parses"))
            });
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::registry()
                .with(filter)
                .with(architect_telemetry::timing::Recorder::default()),
        );
    }
    let mut steps: Vec<Step> = Vec::new();

    let at = Instant::now();
    session_daw::open::open_silent(&path).expect("opens");
    steps.push(Step {
        name: "open",
        ms: at.elapsed().as_secs_f64() * 1e3,
        calls: 1,
        ..Step::default()
    });

    if std::env::var_os("FTS_BENCH_DETECT").is_some() {
        let at = Instant::now();
        let (said, window) = run(
            &path,
            "HITS_DETECT",
            serde_json::json!({ "all_takes": true }),
        );
        steps.push(Step {
            name: "detect",
            ms: at.elapsed().as_secs_f64() * 1e3,
            window_ms: window.as_secs_f64() * 1e3,
            calls: 1,
            note: format!("{} takes", said["takes"]),
        });
    }

    // The comp: every phrase from its best judged take.
    let phrases = phrases(&path);
    let at = Instant::now();
    let (mut window, mut calls, mut comped) = (Duration::ZERO, 0usize, 0usize);
    for (_, from, to) in &phrases {
        let (ranked, w) = run(
            &path,
            "COMP_CANDIDATES",
            serde_json::json!({ "from": from, "to": to }),
        );
        window += w;
        calls += 1;
        if let Some(take) = best_other(&ranked) {
            let (_, w) = run(
                &path,
                "COMP_CHOOSE",
                serde_json::json!({ "from": from, "to": to, "take": take }),
            );
            window += w;
            calls += 1;
            comped += 1;
        }
    }
    steps.push(Step {
        name: "comp",
        ms: at.elapsed().as_secs_f64() * 1e3,
        window_ms: window.as_secs_f64() * 1e3,
        calls,
        note: format!(
            "{} phrases, {comped} comped from another take",
            phrases.len()
        ),
    });

    // The quantize: the whole song, from scratch.
    let at = Instant::now();
    let (said, window) = run(&path, "DRUMS_REQUANTIZE", serde_json::json!({}));
    steps.push(Step {
        name: "quantize",
        ms: at.elapsed().as_secs_f64() * 1e3,
        window_ms: window.as_secs_f64() * 1e3,
        calls: 1,
        note: said.to_string().chars().take(160).collect(),
    });

    // A hand's touch-ups: snare hits across the song, each dragged a little.
    let (daw, project) = session_daw::actions::song().expect("the song");
    let snares: Vec<String> = session_daw::hits::playback(daw, &project)
        .into_iter()
        .filter(|h| h.piece.to_lowercase().contains("snare"))
        .map(|h| h.id)
        .collect();
    let every = (snares.len() / DRAGS).max(1);
    let at = Instant::now();
    let (mut window, mut dragged) = (Duration::ZERO, 0usize);
    for id in snares.iter().step_by(every).take(DRAGS) {
        let (_, w) = run(
            &path,
            "HITS_PLACE",
            serde_json::json!({ "id": id, "by_ms": DRAG_MS, "snap": false }),
        );
        window += w;
        dragged += 1;
    }
    steps.push(Step {
        name: "drags",
        ms: at.elapsed().as_secs_f64() * 1e3,
        window_ms: window.as_secs_f64() * 1e3,
        calls: dragged,
        note: format!("{dragged} snare hits by {DRAG_MS} ms"),
    });

    // What a window then reads back to draw it all (a full read and plan).
    let at = Instant::now();
    let _ = StudioSession::read_current(Some(&path), None).expect("reads back");
    steps.push(Step {
        name: "readback",
        ms: at.elapsed().as_secs_f64() * 1e3,
        calls: 1,
        note: "the whole song read and planned, as a window opening it".to_owned(),
        ..Step::default()
    });

    let edit_ms: f64 = steps
        .iter()
        .filter(|s| matches!(s.name, "comp" | "quantize" | "drags"))
        .map(|s| s.ms)
        .sum();
    // The slowest actions, by stage, from the recorder.
    let slowest: Vec<serde_json::Value> = {
        let mut timed = architect_telemetry::timing::recent(512);
        timed.sort_by(|a, b| b.total_ms.total_cmp(&a.total_ms));
        timed
            .iter()
            .take(8)
            .map(|t| {
                serde_json::json!({
                    "action": t.field("action.id").unwrap_or(t.name),
                    "ms": round(t.total_ms),
                    "stages": t.stages.iter().map(|(k, ms)| format!("{k} {ms:.1}")).collect::<Vec<_>>(),
                })
            })
            .collect()
    };
    let report = serde_json::json!({
        "song": path.file_name().map(|n| n.to_string_lossy().into_owned()),
        "edit_ms": round(edit_ms),
        "steps": steps.iter().map(Step::json).collect::<Vec<_>>(),
        "slowest": slowest,
    });
    if let Some(out) = std::env::var_os("FTS_BENCH_OUT") {
        std::fs::write(&out, serde_json::to_string_pretty(&report).expect("json"))
            .expect("report written");
    }
    let said = serde_json::to_string_pretty(&report).expect("json");
    if let Some(budget) = std::env::var("FTS_BENCH_BUDGET_MS")
        .ok()
        .and_then(|b| b.parse::<f64>().ok())
    {
        assert!(
            edit_ms <= budget,
            "the drum edit took {edit_ms:.0} ms, over {budget} ms:\n{said}"
        );
    }
    assert!(std::env::var_os("FTS_BENCH_SHOW").is_none(), "{said}");
}
