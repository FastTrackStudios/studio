//! `DRUMS_BENCH`: the drum edit's pipeline run from a stage and timed,
//! stage by stage — where the seconds of an edit go.
//!
//! - `from: "nothing"`: the edit lanes reset, the hits found (every
//!   take), planned (`DRUMS_PLAN`, when an extension of the engine has
//!   it), applied and checked;
//! - `from: "hits"`: the markers kept; reset, plan, apply, check;
//! - `from: "targets"` with `list`: reset, then a known change list
//!   applied (`DRUMS_APPLY {list}`) — the engine's apply alone.
//!
//! Each stage runs through the Action List, as an agent runs it, under
//! one span per stage (`DRUMS_BENCH.<stage>`). `repeat` runs the whole
//! set that many times, for the min, median and max. The song is marked
//! first and brought back to the mark after (unless `keep`).

use std::collections::BTreeMap;

use architect::action::{ActionList, ActionMeta, Effect};
use facet::Facet;

use crate::actions::meta;

static BENCH: ActionMeta = meta(
    "DRUMS_BENCH",
    "drums_bench",
    "Time the drum edit's pipeline",
    "Run the drum edit from a stage and time every stage, as an agent would run it: `from: \"nothing\"` resets the edit lanes (DRUMS_EDIT_LANES reset), finds the hits (HITS_DETECT all_takes), plans (DRUMS_PLAN, when loaded; else skipped, and said), applies (DRUMS_APPLY) and checks; `from: \"hits\"` keeps the markers (reset, plan, apply, check); `from: \"targets\"` with `list` (a change list's path) resets and applies the list — the engine's apply alone. `repeat` (1) runs the set that many times: each stage's min, median and max, ms, and the apply's own stages (read, solve, cut, write, crossfade, check, lock). Marks the song first and brings it back to the mark after, unless `keep`. Says a table too.",
    "Drums",
    "",
);

#[derive(Facet)]
struct BenchArg {
    from: Option<String>,
    list: Option<String>,
    repeat: Option<u32>,
    keep: Option<bool>,
}

/// One stage's times.
#[derive(Facet, Debug, Default, Clone, PartialEq)]
pub struct StageTimes {
    pub stage: String,
    pub runs: u32,
    pub min_ms: f64,
    pub median_ms: f64,
    pub max_ms: f64,
}

#[derive(Facet, Debug, Default)]
struct Bench {
    from: String,
    repeat: u32,
    stages: Vec<StageTimes>,
    /// The apply's own stages, the same way.
    apply: Vec<StageTimes>,
    table: String,
    /// What was skipped or failed, and why.
    notes: Vec<String>,
    mark: Option<String>,
    restored: bool,
}

/// `times` (ms) summed up as min, median and max.
#[must_use]
pub fn summary(stage: &str, times: &[f64]) -> StageTimes {
    let mut t = times.to_vec();
    t.sort_by(f64::total_cmp);
    let r = |x: f64| (x * 10.0).round() / 10.0;
    StageTimes {
        stage: stage.to_owned(),
        runs: u32::try_from(t.len()).unwrap_or(u32::MAX),
        min_ms: r(t.first().copied().unwrap_or(0.0)),
        median_ms: r(t.get(t.len() / 2).copied().unwrap_or(0.0)),
        max_ms: r(t.last().copied().unwrap_or(0.0)),
    }
}

/// The stages as a table, one line each.
#[must_use]
pub fn table(stages: &[StageTimes]) -> String {
    let mut out = String::from("stage            runs     min ms  median ms     max ms\n");
    for s in stages {
        out.push_str(&format!(
            "{:<16} {:>4} {:>10.1} {:>10.1} {:>10.1}\n",
            s.stage, s.runs, s.min_ms, s.median_ms, s.max_ms
        ));
    }
    out
}

/// The targets a plan's answer gives: `targets` as {id: seconds}, or
/// columnar `ids` with `targets`/`to`.
#[must_use]
pub fn plan_targets(
    plan: &serde_json::Value,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    if let Some(map) = plan.get("targets").and_then(serde_json::Value::as_object) {
        return Some(map.clone());
    }
    let ids = plan.get("ids")?.as_array()?;
    let to = plan.get("targets").or_else(|| plan.get("to"))?.as_array()?;
    Some(
        ids.iter()
            .zip(to)
            .filter_map(|(id, t)| Some((id.as_str()?.to_owned(), serde_json::json!(t.as_f64()?))))
            .collect(),
    )
}

fn run(id: &str, args: &serde_json::Value) -> Result<(serde_json::Value, f64), String> {
    let span = tracing::info_span!("DRUMS_BENCH.stage", stage = id, ms = tracing::field::Empty);
    let _in = span.enter();
    let t = web_time::Instant::now();
    let said = crate::actions::list()
        .run_json(id, args.to_string().as_bytes())
        .map_err(|e| format!("{id}: {e}"))?;
    let ms = t.elapsed().as_secs_f64() * 1_000.0;
    span.record("ms", ms);
    Ok((
        serde_json::from_slice(&said).unwrap_or(serde_json::Value::Null),
        ms,
    ))
}

#[allow(clippy::too_many_lines)]
fn bench(a: BenchArg) -> Result<Bench, String> {
    let from = a.from.unwrap_or_else(|| "targets".to_owned());
    let repeat = a.repeat.unwrap_or(1).clamp(1, 50);
    let span =
        tracing::info_span!("DRUMS_BENCH", from = %from, repeat, total_ms = tracing::field::Empty);
    let _in = span.enter();
    let began = web_time::Instant::now();
    let (_, project) = crate::actions::song()?;
    let daw::service::ProjectContext::Project(guid) = &project else {
        return Err("no song open".to_owned());
    };
    let stages: &[&str] = match from.as_str() {
        "nothing" => &["reset", "detect", "plan", "apply"],
        "hits" => &["reset", "plan", "apply"],
        "targets" => &["reset", "apply"],
        other => return Err(format!("from {other:?}: one of nothing, hits, targets")),
    };
    if from == "targets" && a.list.is_none() {
        return Err("from targets needs `list`: a change list's path".to_owned());
    }
    let has_plan = crate::actions::list().info("DRUMS_PLAN").is_some();
    let mut out = Bench {
        from: from.clone(),
        repeat,
        ..Bench::default()
    };
    if stages.contains(&"plan") && !has_plan {
        out.notes.push(
            "DRUMS_PLAN is not loaded: the plan stage is skipped, the apply has no targets"
                .to_owned(),
        );
    }
    let step = tracing::info_span!("DRUMS_BENCH.mark").entered();
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mark = crate::project_file::mark(guid, BENCH.id, |taken| {
        crate::drum_apply::unique(&format!("Before DRUMS_BENCH {stamp}"), taken)
    })?;
    out.mark = Some(mark.clone());
    drop(step);

    let mut times: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    let mut applied: BTreeMap<&'static str, Vec<f64>> = BTreeMap::new();
    let result = (|| -> Result<(), String> {
        for _ in 0..repeat {
            let mut targets: Option<serde_json::Map<String, serde_json::Value>> = None;
            for stage in stages {
                let (said, ms) = match *stage {
                    "reset" => run("DRUMS_EDIT_LANES", &serde_json::json!({"reset": true}))?,
                    "detect" => run("HITS_DETECT", &serde_json::json!({"all_takes": true}))?,
                    "plan" if has_plan => {
                        let (said, ms) = run("DRUMS_PLAN", &serde_json::json!({}))?;
                        targets = plan_targets(&said);
                        if targets.is_none() {
                            out.notes.push("DRUMS_PLAN's answer has no targets the bench reads (`targets` {id: s}, or `ids` with `targets`)".to_owned());
                        }
                        (said, ms)
                    }
                    "plan" => continue,
                    "apply" => {
                        let args = match (&a.list, &targets) {
                            (Some(list), _) if from == "targets" => {
                                serde_json::json!({"list": list})
                            }
                            (_, Some(t)) => serde_json::json!({"targets": t}),
                            _ => {
                                out.notes
                                    .push("nothing to apply: no list and no plan".to_owned());
                                continue;
                            }
                        };
                        run(crate::drum_apply::APPLY.id, &args)?
                    }
                    _ => continue,
                };
                times.entry(stage).or_default().push(ms);
                if *stage == "apply" {
                    let stages = said.get("ms");
                    for key in [
                        "read",
                        "solve",
                        "history",
                        "cut",
                        "write",
                        "crossfade",
                        "check",
                        "lock",
                        "total",
                    ] {
                        if let Some(v) = stages
                            .and_then(|s| s.get(key))
                            .and_then(serde_json::Value::as_f64)
                        {
                            applied.entry(key).or_default().push(v);
                        }
                    }
                    if let Some(why) = said.get("refused").and_then(serde_json::Value::as_str) {
                        out.notes.push(format!("the apply was refused: {why}"));
                    }
                }
            }
        }
        Ok(())
    })();
    if let Err(why) = result {
        out.notes.push(why);
    }
    // "check" is the apply's own: its result carries the check.
    out.stages = stages
        .iter()
        .filter_map(|s| times.get(s).map(|t| summary(s, t)))
        .collect();
    out.apply = applied.iter().map(|(k, t)| summary(k, t)).collect();
    if !a.keep.unwrap_or(false) {
        let step = tracing::info_span!("DRUMS_BENCH.restore").entered();
        let t = web_time::Instant::now();
        match run(
            "SESSION_HISTORY_RESTORE",
            &serde_json::json!({"mark": mark}),
        ) {
            Ok(_) => {
                out.restored = true;
                out.stages
                    .push(summary("restore", &[t.elapsed().as_secs_f64() * 1_000.0]));
            }
            Err(why) => out
                .notes
                .push(format!("the song was not brought back to {mark}: {why}")),
        }
        drop(step);
    }
    let mut all = out.stages.clone();
    all.extend(out.apply.iter().map(|s| StageTimes {
        stage: format!("apply.{}", s.stage),
        ..s.clone()
    }));
    out.table = table(&all);
    span.record("total_ms", began.elapsed().as_secs_f64() * 1_000.0);
    Ok(out)
}

/// Register `DRUMS_BENCH`.
///
/// # Errors
/// When it is already registered.
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&BENCH, Effect::Mutating, bench)?;
    Ok(())
}
