//! The whole drum edit of a real song, timed end to end: the kit comped
//! from its best takes phrase by phrase in one edit, quantized over the whole song,
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

/// The hits that play twice within 100 ms of themselves — a flam an edit
/// made: a seam that left one attack in two pieces. (A hit heard twice far
/// apart is a recording used twice in the song, as it was played.)
fn flams() -> Vec<String> {
    let (daw, project) = session_daw::actions::song().expect("the song");
    let mut by_id: std::collections::HashMap<String, Vec<f64>> = std::collections::HashMap::new();
    for h in session_daw::hits::playback(daw, &project) {
        by_id.entry(h.id).or_default().push(h.time);
    }
    let mut out: Vec<String> = by_id
        .into_iter()
        .filter_map(|(id, mut times)| {
            times.sort_by(f64::total_cmp);
            times
                .windows(2)
                .any(|w| w[1] - w[0] < 0.1)
                .then(|| format!("{id} at {times:.3?}"))
        })
        .collect();
    out.sort();
    out
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

    let flams_opened = flams();

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

    // The comp: every phrase from its best judged take, as one edit (what
    // Ctrl+T does over the whole song).
    let at = Instant::now();
    let (said, window) = run(&path, "COMP_BEST", serde_json::json!({}));
    let comped = said["comped"].as_array().map_or(0, Vec::len);
    steps.push(Step {
        name: "comp",
        ms: at.elapsed().as_secs_f64() * 1e3,
        window_ms: window.as_secs_f64() * 1e3,
        calls: 1,
        note: format!(
            "{} phrases, {comped} comped from another take, {} item edits",
            said["phrases"], said["ops"]
        ),
    });
    // Comped, every phrase plays its best take: nothing left to comp.
    let (again, _) = run(&path, "COMP_BEST", serde_json::json!({ "dry_run": true }));
    let left = again["comped"].as_array().cloned().unwrap_or_default();
    let first = left.first().map(|c| {
        let (ranked, _) = run(
            &path,
            "COMP_CANDIDATES",
            serde_json::json!({ "from": c["from"], "to": c["to"] }),
        );
        let (daw, project) = session_daw::actions::song().expect("the song");
        let (from, to) = (
            c["from"].as_f64().unwrap_or(0.0),
            c["to"].as_f64().unwrap_or(0.0),
        );
        let versions = session_daw::hits::takes_over(daw, &project, (from, to));
        let files: Vec<(String, Vec<String>)> = versions
            .iter()
            .map(|v| {
                (
                    v.pass.clone(),
                    v.files.iter().map(|(_, f)| f.clone()).collect(),
                )
            })
            .collect();
        let chosen = said["comped"]
            .as_array()
            .and_then(|all| all.iter().find(|d| d["from"] == c["from"]))
            .cloned();
        let _ = files;
        // What each mic's COMP lane plays at the phrase's middle.
        let mid = f64::midpoint(from, to);
        let comp_now: Vec<String> = daw::service::Tracks::all(daw, project.clone())
            .into_iter()
            .filter_map(|t| {
                let comp = session_daw::drum_lanes::lane_named(&t, session_daw::drum_lanes::COMP)?;
                let there: Vec<String> = daw::service::Items::get_items(
                    daw,
                    project.clone(),
                    daw::service::TrackRef::Guid(t.guid.clone()),
                )
                .into_iter()
                .filter(|i| {
                    let a = i.position.as_seconds();
                    a <= mid && mid < a + i.length.as_seconds()
                })
                .map(|i| {
                    let file = daw::service::Takes::get_active_take(
                        daw,
                        project.clone(),
                        daw::service::ItemRef::Guid(i.guid.clone()),
                    )
                    .and_then(|t| t.source_file_path)
                    .and_then(|f| f.rsplit('/').next().map(str::to_owned))
                    .unwrap_or_default();
                    format!("lane {:?} {file}", i.fixed_lane)
                })
                .collect();
                let comping = daw::service::Tracks::comping(
                    daw,
                    project.clone(),
                    daw::service::TrackRef::Guid(t.guid.clone()),
                )
                .ok()
                .and_then(|c| c.comp_lane);
                Some(format!(
                    "{} (lane named COMP {comp}, comping says {comping:?}): {there:?}",
                    t.name
                ))
            })
            .take(4)
            .collect();
        format!("{c}\nthe first call chose there: {chosen:?}\nthe COMP lanes there: {comp_now:#?}")
    });
    assert!(
        left.is_empty(),
        "comped, the best takes play; {} phrases still want comping, the first: {}",
        left.len(),
        first.unwrap_or_default()
    );

    let flams_comped = flams();

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

    let flams_quantized = flams();

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

    let flams_dragged = flams();

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
    "flams": {
            "opened": flams_opened, "comped": flams_comped,
            "quantized": flams_quantized, "dragged": flams_dragged,
        },
    });
    if let Some(out) = std::env::var_os("FTS_BENCH_OUT") {
        std::fs::write(&out, serde_json::to_string_pretty(&report).expect("json"))
            .expect("report written");
    }
    let said = serde_json::to_string_pretty(&report).expect("json");
    // No step leaves a hit playing twice that the step before did not.
    let new = |before: &[String], after: &[String]| -> Vec<String> {
        after
            .iter()
            .filter(|f| !before.contains(f))
            .cloned()
            .collect()
    };
    for (step, before, after) in [
        ("comp", &flams_opened, &flams_comped),
        ("quantize", &flams_comped, &flams_quantized),
        ("drags", &flams_quantized, &flams_dragged),
    ] {
        let made = new(before, after);
        assert!(
            made.is_empty(),
            "the {step} left {} hits playing twice: {made:#?}\n{said}",
            made.len()
        );
    }
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
