//! Scores the planner against a finished, approved drum edit.
//!
//! `cargo run --release -p rhythm-editing --example plan_bench -- <request.json> <edit.json>`
//!
//! - `request.json`: a `daw.edit` `PlanRequest` (Studio's `DRUMS_PLAN
//!   {dump: <path>}` writes one), the hits as recorded.
//! - `edit.json`: the edit as approved, `studio.drum-edit/1`: every hit that
//!   plays with its `piece`, `take`, `recorded` and `to` (project seconds).
//!
//! A planned hit matches when its target is within 2 ms of the edit's `to`
//! (a hit the plan removes or deletes misses). Prints the share, the
//! misses by bar (and whether a doubt covers each), the doubts and the
//! planner's time. Song data stays where it is: nothing is written.

use std::collections::{BTreeMap, HashSet};
use std::time::Instant;

use serde::Deserialize;

#[derive(Deserialize)]
struct Edit {
    tempo: Tempo,
    hits: Vec<EditHit>,
}

#[derive(Deserialize)]
struct Tempo {
    bpm: f64,
    bar_1_at: f64,
}

#[derive(Deserialize)]
struct EditHit {
    piece: String,
    take: String,
    recorded: f64,
    to: f64,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(req), Some(edit)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: plan_bench <request.json> <edit.json> [--misses]");
        std::process::exit(2);
    };
    let show = args.iter().any(|a| a == "--misses");
    let request: daw_edit::PlanRequest =
        serde_json::from_slice(&std::fs::read(req).expect("request")).expect("a PlanRequest");
    let edit: Edit = serde_json::from_slice(&std::fs::read(edit).expect("edit")).expect("studio.drum-edit/1");

    let started = Instant::now();
    let plan = rhythm_editing::plan::plan(&request);
    let took = started.elapsed();

    let bar_len = 240.0 / edit.tempo.bpm;
    let at = |t: f64| {
        let b = (t - edit.tempo.bar_1_at) / bar_len;
        let bar = b.floor();
        format!("{}.{:.2}", bar as i64 + 1, (b - bar) * 4.0 + 1.0)
    };
    let planned: BTreeMap<&str, &daw_edit::plan::Planned> = plan.hits.iter().map(|p| (p.id.as_str(), p)).collect();
    let gone: HashSet<&str> = plan.remove.iter().chain(&plan.delete).map(String::as_str).collect();
    let doubted: HashSet<&str> = plan.doubts.iter().map(|d| d.id.as_str()).collect();
    let (mut ok, mut total, mut covered) = (0usize, 0usize, 0usize);
    let mut by_bar: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let mut unmatched = 0usize;
    for e in &edit.hits {
        let Some(h) = request
            .hits
            .iter()
            .find(|h| h.piece == e.piece && h.take == e.take && (h.time - e.recorded).abs() <= 0.002)
        else {
            unmatched += 1;
            continue;
        };
        total += 1;
        let p = planned[h.id.as_str()];
        if !gone.contains(h.id.as_str()) && (p.target - e.to).abs() <= 0.002 {
            ok += 1;
            continue;
        }
        let doubt = doubted.contains(h.id.as_str());
        covered += usize::from(doubt);
        let bar = ((e.to - edit.tempo.bar_1_at) / bar_len).floor() as i64 + 1;
        by_bar.entry(bar).or_default().push(format!(
            "{} {} {} → {} ({:+.1} ms; {}{})",
            e.piece,
            at(e.recorded),
            at(p.target),
            at(e.to),
            (p.target - e.to) * 1000.0,
            p.reason,
            if doubt { ", doubted" } else { "" }
        ));
    }
    println!(
        "planned {} hits in {:.1} ms; {ok}/{total} within 2 ms of the edit ({:.1}%); {} misses, {covered} of them doubted; {} doubts; {} removed, {} deleted, {} rides; {unmatched} edit hits not in the request",
        request.hits.len(),
        took.as_secs_f64() * 1000.0,
        100.0 * ok as f64 / total.max(1) as f64,
        total - ok,
        plan.doubts.len(),
        plan.remove.len(),
        plan.delete.len(),
        plan.rides.len(),
    );
    if args.iter().any(|a| a == "--leans") {
        // The owner's lean per half bar: the median of recorded minus `to`
        // over the hits that moved.
        let mut owner: BTreeMap<(i64, u8), Vec<f64>> = BTreeMap::new();
        for e in &edit.hits {
            if (e.to - e.recorded).abs() < 0.0008 {
                continue;
            }
            let b = (e.to - edit.tempo.bar_1_at) / bar_len;
            let half = u8::from(b - b.floor() >= 0.5);
            owner.entry((b.floor() as i64 + 1, half)).or_default().push(e.recorded - e.to);
        }
        for l in &plan.leans {
            if let Some(v) = owner.get_mut(&(i64::from(l.bar), l.half)) {
                v.sort_by(f64::total_cmp);
                let m = v[v.len() / 2];
                let flag = if (m - l.lean).abs() > 0.02 { "  <-" } else { "" };
                println!("lean {}.{} plan {:+.0} owner {:+.0} ({} anchors){flag}", l.bar, l.half, l.lean * 1000.0, m * 1000.0, l.anchors);
            }
        }
    }
    let bars: Vec<String> = by_bar.iter().map(|(b, v)| format!("{b}×{}", v.len())).collect();
    println!("misses by bar: {}", bars.join(" "));
    if show {
        for (b, v) in &by_bar {
            for m in v {
                println!("  bar {b}: {m}");
            }
        }
    }
}
