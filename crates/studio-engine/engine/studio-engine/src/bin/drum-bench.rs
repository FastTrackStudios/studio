//! `drum-bench` — `DRUMS_APPLY`'s solve timed headless, without Studio
//! or a song open.
//!
//! - `drum-bench [--hits N] [--repeat R]`: a synthetic song's edit (N hits,
//!   1,400 unless given) matched and solved R times (20): min, median and
//!   max per stage.
//! - `drum-bench --list <file.json>`: a real change list
//!   (`studio.drum-edit/1`, e.g. a finished edit exported by
//!   `DRUMS_EDIT_EXPORT`), its hits taken as the markers a fresh detection
//!   gives (each where it was recorded, the edit reset), matched back to
//!   the list and solved: what `DRUMS_APPLY {list}` decides before it
//!   touches the song, and the conflicts it would refuse.
//!
//! The placement and the write need the song's lanes and the `daw.edit`
//! provider (rhythm), so they are timed by `DRUMS_BENCH` in a running
//! engine, not here.

use std::time::Instant;

use studio_engine::drum_apply::{Edit, Limits, Stroke, solve, synthetic};
use studio_engine::edit_list::{match_list, read};

fn stats(mut ms: Vec<f64>) -> (f64, f64, f64) {
    ms.sort_by(f64::total_cmp);
    (
        ms.first().copied().unwrap_or(0.0),
        ms.get(ms.len() / 2).copied().unwrap_or(0.0),
        ms.last().copied().unwrap_or(0.0),
    )
}

fn row(stage: &str, ms: Vec<f64>) {
    let runs = ms.len();
    let (lo, mid, hi) = stats(ms);
    println!("{stage:<10} {runs:>4} {lo:>10.3} {mid:>10.3} {hi:>10.3}");
}

fn main() -> eyre::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let repeat: usize = value("--repeat").and_then(|v| v.parse().ok()).unwrap_or(20);
    let n: usize = value("--hits")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_400);
    let limits = Limits::default();

    let (mut matching, mut solving) = (Vec::new(), Vec::new());
    let mut last = None;
    if let Some(path) = value("--list") {
        let list = read(std::path::Path::new(&path)).map_err(|e| eyre::eyre!(e))?;
        // The markers a detection gives: each hit where it was recorded,
        // an id of its own with the list's take code.
        let strokes: Vec<Stroke> = list
            .hits
            .iter()
            .enumerate()
            .map(|(k, h)| Stroke {
                id: format!("{}.{k:04}~{}", h.piece.to_lowercase(), h.take),
                piece: h.piece.clone(),
                now: h.recorded,
                recorded: h.recorded,
                peak_db: -6.0,
                flam_with: None,
            })
            .collect();
        println!("{path}: {} hits", strokes.len());
        for _ in 0..repeat {
            let t = Instant::now();
            let m = match_list(&list, &strokes);
            matching.push(t.elapsed().as_secs_f64() * 1_000.0);
            let edit = Edit {
                targets: m.targets.clone(),
                remove: m.removed.iter().cloned().collect(),
                ..Edit::default()
            };
            let t = Instant::now();
            let s = solve(&strokes, &edit, limits);
            solving.push(t.elapsed().as_secs_f64() * 1_000.0);
            last = Some((m.targets.len(), m.unmatched.len(), m.removed.len(), s));
        }
        if let Some((matched, unmatched, removed, _)) = &last {
            println!("matched {matched}, unmatched {unmatched}, removed {removed}");
        }
    } else {
        let (strokes, edit) = synthetic(n);
        println!(
            "synthetic: {} hits, {} targets",
            strokes.len(),
            edit.targets.len()
        );
        for _ in 0..repeat {
            let t = Instant::now();
            let s = solve(&strokes, &edit, limits);
            solving.push(t.elapsed().as_secs_f64() * 1_000.0);
            last = Some((edit.targets.len(), 0, 0, s));
        }
    }
    println!("stage      runs     min ms  median ms     max ms");
    if !matching.is_empty() {
        row("match", matching);
    }
    row("solve", solving);
    if let Some((_, _, _, s)) = last {
        println!(
            "moments {}, moved {}, conflicts {}",
            s.moments.len(),
            s.moved(),
            s.conflicts.len()
        );
        let mut kinds: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for c in &s.conflicts {
            *kinds.entry(c.kind.as_str()).or_default() += 1;
        }
        println!("conflicts by kind: {kinds:?}");
        for c in s.conflicts.iter().take(12) {
            println!("  {} at {:.3}: {:?} — {}", c.kind, c.at, c.ids, c.why);
        }
    }
    Ok(())
}
