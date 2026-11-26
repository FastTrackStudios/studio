//! What a person or agent does by hand on the song Unbreakable — open the
//! set and the song, play, listen to the snare, A/B an EQ band, render the
//! song — run against the real engine binary and timed, so Studio can be
//! optimised by running one test instead of driving the app.
//!
//! ```sh
//! just test-unbreakable      # the real song (skipped where it is not), then the synthetic one
//! ```
//!
//! The engine is `daw-extension --engine` (this package's binary), started
//! headless in a sandbox of its own (`engine.rs`) and driven only through
//! its MCP door, as an agent drives it: nothing of the engine is linked,
//! so the test outlives the engine's internals moving. Its spans come to
//! a receiver the test owns (`otlp.rs`).
//!
//! Each step is held to its budget in `budgets.toml`. Every run writes its
//! record to `target/<triple>/<profile>/unbreakable/`, and a run on the
//! real song appends a line to `docs/perf/unbreakable.jsonl`.
//!
//! - **`unbreakable`**: the owner's song, copied (`song.rs`); skipped
//!   with the reason when it is not on this machine (`FTS_UNBREAKABLE`
//!   names another file, `off` skips it). `#[ignore]`d so `just test
//!   engine` does not spend minutes on it; `just test-unbreakable` runs it.
//! - **`synthetic_song_runs_the_same_sequence`**: a few seconds of
//!   generated drums, the same sequence, everywhere CI runs.

mod budgets;
mod engine;
mod mcp;
mod otlp;
mod report;
mod sequence;
mod shape;
mod song;

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::{Value, json};

use budgets::{Budgets, Verdict};
use sequence::{Change, Plan, Run};

fn engine_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_daw-extension"))
}

fn budgets(song: &str) -> Budgets {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/unbreakable/budgets.toml");
    let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    Budgets::parse(&text, song).unwrap_or_else(|e| panic!("{e}"))
}

/// A clean folder for this run's files, the song's earlier runs removed
/// (each holds a copy of the song and a render).
fn run_dir(song: &str, unix: u64) -> PathBuf {
    let runs = report::profile_dir().join("unbreakable").join("runs");
    if let Ok(entries) = std::fs::read_dir(&runs) {
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("{song}-"))
            {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    let dir = runs.join(format!("{song}-{unix}"));
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    dir
}

#[test]
#[ignore = "drives the owner's song for minutes: `just test-unbreakable`"]
fn unbreakable() {
    let original = match song::unbreakable() {
        Ok(path) => path,
        Err(why) => {
            eprintln!("SKIPPED unbreakable: {why}");
            return;
        }
    };
    let began = Instant::now();
    let (date, unix) = report::now();
    let cache = report::profile_dir().join("unbreakable").join("song-cache");
    let (copy, source, fresh) =
        song::cached(&original, &cache).unwrap_or_else(|e| panic!("the song's copy: {e}"));
    let prepare_ms = began.elapsed().as_secs_f64() * 1e3;
    let dir = run_dir("unbreakable", unix);
    let setlist = song::stage_unbreakable(&copy, &original, &dir).unwrap_or_else(|e| panic!("{e}"));
    let title = original
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let plan = Plan {
        song: "unbreakable",
        setlist,
        title,
        track: "Drums/Snare".into(),
        reference: Some("REFERENCES/REHEARSAL/Drums/Snare - Unbreakable".into()),
        listen: ("CH 1".into(), "CH 1 end".into()),
        change: Change::SignalEq,
        render: ("IN".into(), "OUT end".into()),
        setup: Vec::new(),
        run_dir: dir.clone(),
    };
    let context = json!({
        "song": original.display().to_string(),
        "setlist_mirrored": song::CRESCENDUM,
        "copy": {"of": source.json(), "fresh": fresh, "prepare_ms": otlp::round(prepare_ms)},
    });
    drive(&plan, date, unix, began, context, true);
}

#[test]
fn synthetic_song_runs_the_same_sequence() {
    let began = Instant::now();
    let (date, unix) = report::now();
    let dir = run_dir("synthetic", unix);
    let setlist = song::stage_synthetic(&dir).unwrap_or_else(|e| panic!("{e}"));
    // Signal where this machine has it, so the render goes through a
    // hosted CLAP; the A/B moves a pan, which any song can.
    let signal = dirs::home_dir()
        .map(|h| h.join(".clap").join("Signal Dev.clap"))
        .filter(|p| p.exists());
    let mut setup = vec![(
        "FX_ADD".to_owned(),
        json!({"track": "Drums/Snare", "fx": "FTS EQ"}),
    )];
    if signal.is_some() {
        setup.push((
            "FX_ADD".to_owned(),
            json!({"track": "Drums/Kick", "fx": "Signal Dev"}),
        ));
    }
    let plan = Plan {
        song: "synthetic",
        setlist,
        title: "synth".into(),
        track: "Drums/Snare".into(),
        // Opening applies the golden template, which names the folder
        // `Reference`: the track by its tail.
        reference: Some("Snare Ref".into()),
        listen: ("CH 1".into(), "CH 1 end".into()),
        change: Change::Pan,
        render: ("IN".into(), "OUT end".into()),
        setup,
        run_dir: dir,
    };
    let context = json!({"signal": signal.map(|p| p.display().to_string())});
    drive(&plan, date, unix, began, context, false);
}

/// Start an engine, run `plan`, judge it, write the record (and the
/// trend line when `trend`), say it, and fail if anything failed.
fn drive(plan: &Plan, date: String, unix: u64, began: Instant, context: Value, trend: bool) {
    let budgets = budgets(plan.song);
    let spans = otlp::Receiver::start();
    let instance = format!("{}-{}", plan.song, std::process::id());
    let started = Instant::now();
    let mut engine = engine::Engine::start(&engine_bin(), &spans.endpoint, &instance)
        .unwrap_or_else(|e| panic!("the engine: {e}"));
    let start_ms = started.elapsed().as_secs_f64() * 1e3;
    let run = sequence::run(&engine, &spans, plan, &budgets.expect);
    let tail = engine.tail(60);
    engine.stop();
    let total_ms = began.elapsed().as_secs_f64() * 1e3;

    let mut failures: Vec<String> = run.errors.clone();
    let mut hints = Vec::new();
    let steps: Vec<Value> = run
        .steps
        .iter()
        .map(|step| {
            let verdict = budgets.judge(step.name, step.ms());
            match &verdict {
                Verdict::Over { budget_ms, limit_ms } => failures.push(format!(
                    "{} took {:.0} ms: over its budget of {budget_ms} ms (limit {limit_ms:.0} ms)",
                    step.name,
                    step.ms()
                )),
                Verdict::Tighten { budget_ms, suggest_ms } => hints.push(format!(
                    "{} took {:.0} ms against a budget of {budget_ms} ms: tighten it to {suggest_ms} in budgets.toml",
                    step.name,
                    step.ms()
                )),
                Verdict::Within { .. } | Verdict::Unbudgeted => {}
            }
            json!({
                "name": step.name,
                "ms": otlp::round(step.ms()),
                "wall_ms": otlp::round(step.wall_ms),
                "engine_ms": step.engine_ms.map(otlp::round),
                "budget_ms": budgets.budget_ms.get(step.name),
                "verdict": verdict.word(),
                "detail": step.detail,
                "spans": step.spans,
            })
        })
        .collect();
    for check in run.checks.iter().filter(|c| !c.ok) {
        failures.push(format!("check {}: {}", check.name, check.said));
    }
    let checks: Vec<Value> = run
        .checks
        .iter()
        .map(|c| json!({"name": c.name, "ok": c.ok, "value": c.value, "said": c.said}))
        .collect();
    let to_store: Vec<String> = run
        .measured
        .iter()
        .filter(|(name, _)| !budgets.expect.contains_key(*name))
        .map(|(name, value)| format!("{name} = {value:.2}"))
        .collect();
    let record = json!({
        "song": plan.song,
        "date": date,
        "commit": report::commit(),
        "machine": report::machine(),
        "engine": {"start_ms": otlp::round(start_ms), "pid": engine.pid, "mcp": engine.mcp_url},
        "total_ms": otlp::round(total_ms),
        "steps": steps,
        "open": {"root": run.open_root, "breakdown": run.open_breakdown},
        "checks": checks,
        "spans": {"received": spans.mark(), "unreadable": spans.unreadable()},
        "context": context,
        "failures": failures,
        "hints": hints,
        "store": to_store,
    });
    let file = report::write_record(plan.song, unix, &record);
    let trend_file = trend.then(|| report::append_trend(&trend_line(&record)));
    say(
        &record,
        &run,
        file.as_ref().ok(),
        trend_file.as_ref().and_then(|r| r.as_ref().ok()),
    );
    if let Err(e) = &file {
        eprintln!("the record could not be written: {e}");
    }
    assert!(
        failures.is_empty(),
        "{} failed:\n  {}\n--- the engine's last lines ---\n{tail}",
        plan.song,
        failures.join("\n  ")
    );
}

/// The trend's one line: the step times, the open's named stages, the
/// checks' values.
fn trend_line(record: &Value) -> Value {
    let pick = |name: &str| {
        record["open"]["breakdown"]
            .as_array()
            .and_then(|rows| rows.iter().find(|r| r["name"] == json!(name)))
            .map(|r| r["total_ms"].clone())
    };
    let stages: serde_json::Map<String, Value> = [
        "song.open",
        "open.engine",
        "load.store",
        "store.read_snapshot",
        "store.unpack",
        "store.import_snapshot",
        "bridge.read_doc",
        "bridge.apply",
        "fx.load_states",
        "open.prepare",
        "open.read",
    ]
    .iter()
    .filter_map(|n| pick(n).map(|v| ((*n).to_owned(), v)))
    .collect();
    let steps: serde_json::Map<String, Value> = record["steps"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| {
            (
                s["name"].as_str().unwrap_or("?").to_owned(),
                s["ms"].clone(),
            )
        })
        .collect();
    let checks: serde_json::Map<String, Value> = record["checks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["value"].is_number())
        .map(|c| {
            (
                c["name"].as_str().unwrap_or("?").to_owned(),
                c["value"].clone(),
            )
        })
        .collect();
    json!({
        "date": record["date"],
        "commit": record["commit"],
        "machine": record["machine"],
        "total_ms": record["total_ms"],
        "steps_ms": steps,
        "open_ms": stages,
        "checks": checks,
        "ok": record["failures"].as_array().is_none_or(Vec::is_empty),
    })
}

/// The run, for the person reading the test's output.
fn say(record: &Value, run: &Run, file: Option<&PathBuf>, trend: Option<&PathBuf>) {
    let mut out = String::new();
    out.push_str(&format!(
        "\n=== {} — {} at {} on {} (load {}) ===\n",
        record["song"].as_str().unwrap_or("?"),
        record["commit"]["sha"].as_str().unwrap_or("?"),
        record["date"].as_str().unwrap_or("?"),
        record["machine"]["host"].as_str().unwrap_or("?"),
        record["machine"]["load_1m"]
    ));
    out.push_str(&format!(
        "{:<14} {:>10} {:>10} {:>10}  verdict\n",
        "step", "engine ms", "wall ms", "budget"
    ));
    for s in record["steps"].as_array().into_iter().flatten() {
        let cell = |v: &Value| {
            v.as_f64()
                .map_or_else(|| "-".to_owned(), |n| format!("{n:.1}"))
        };
        out.push_str(&format!(
            "{:<14} {:>10} {:>10} {:>10}  {}\n",
            s["name"].as_str().unwrap_or("?"),
            cell(&s["engine_ms"]),
            cell(&s["wall_ms"]),
            cell(&s["budget_ms"]),
            s["verdict"].as_str().unwrap_or("?")
        ));
    }
    out.push_str("open, by span (total ms):\n");
    for row in run.open_breakdown.iter().take(16) {
        out.push_str(&format!(
            "  {:<28} {:>10.1}  ×{}\n",
            row["name"].as_str().unwrap_or("?"),
            row["total_ms"].as_f64().unwrap_or(0.0),
            row["count"]
        ));
    }
    for c in &run.checks {
        out.push_str(&format!(
            "check {:<20} {}  {}\n",
            c.name,
            if c.ok { "ok  " } else { "FAIL" },
            c.said
        ));
    }
    for line in record["hints"].as_array().into_iter().flatten() {
        out.push_str(&format!("hint: {}\n", line.as_str().unwrap_or_default()));
    }
    for line in record["store"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "store in budgets.toml [{}.expect]: {}\n",
            record["song"].as_str().unwrap_or("?"),
            line.as_str().unwrap_or_default()
        ));
    }
    out.push_str(&format!(
        "total {:.1} s",
        record["total_ms"].as_f64().unwrap_or(0.0) / 1e3
    ));
    if let Some(file) = file {
        out.push_str(&format!("; record {}", file.display()));
    }
    if let Some(trend) = trend {
        out.push_str(&format!("; trend {}", trend.display()));
    }
    eprintln!("{out}");
}
