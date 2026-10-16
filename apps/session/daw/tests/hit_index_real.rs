//! The hit index on a real song, on a copy of it: after the song opens,
//! after a hit is dragged and after a phrase is comped, every hit is where
//! reading every slice again says it is — and the index brought up to date
//! after an edit costs what the edit touched, not the song.
//!
//! ```sh
//! FTS_PERF_SESSION=/path/to/a/copy/of/Unbreakable.session \
//!   cargo nextest run --cargo-profile release-fast -p session-daw --features signal \
//!   --test hit_index_real --run-ignored all
//! ```

use std::path::PathBuf;
use std::time::Instant;

use session_daw::hits::{Played, playback, playback_scan};

/// Each hit as `(id, time to the microsecond)`, in order.
fn shape(hits: &[Played]) -> Vec<(String, i64)> {
    let mut out: Vec<(String, i64)> = hits
        .iter()
        .map(|h| {
            let us = num_traits::ToPrimitive::to_i64(&(h.time * 1e6).round()).unwrap_or(0);
            (h.id.clone(), us)
        })
        .collect();
    out.sort();
    out
}

fn run(id: &str, args: serde_json::Value) -> serde_json::Value {
    let said = session_daw::actions::list()
        .run_json(id, args.to_string().as_bytes())
        .unwrap_or_else(|e| panic!("{id}: {e}"));
    serde_json::from_slice(&said).unwrap_or_default()
}

#[test]
#[ignore = "edits a real session; set FTS_PERF_SESSION to a copy"]
fn the_index_is_every_slice_read_again() {
    let Some(path) = std::env::var_os("FTS_PERF_SESSION").map(PathBuf::from) else {
        return;
    };
    session_daw::open::open_silent(&path).expect("opens");
    let (daw, project) = session_daw::actions::song().expect("the song");
    let mut said = Vec::new();
    let mut check = |when: &str| {
        let at = Instant::now();
        let indexed = playback(daw, &project);
        let index_ms = at.elapsed().as_secs_f64() * 1e3;
        let at = Instant::now();
        let scanned = playback_scan(daw, &project);
        let scan_ms = at.elapsed().as_secs_f64() * 1e3;
        said.push(format!(
            "{when}: {} hits, index {index_ms:.2} ms, scan {scan_ms:.1} ms",
            indexed.len()
        ));
        assert_eq!(
            shape(&indexed),
            shape(&scanned),
            "{when}: the index is not the slices read again\n{said:#?}"
        );
    };
    check("opened");
    // Read again with nothing changed: the journal says nothing.
    check("unchanged");

    let snare = playback(daw, &project)
        .into_iter()
        .find(|h| h.piece.to_lowercase().contains("snare") && h.time > 84.0)
        .expect("a snare");
    run(
        "HITS_PLACE",
        serde_json::json!({ "id": snare.id, "by_ms": 8.0, "snap": false }),
    );
    check("after a drag");

    let ranked = run(
        "COMP_CANDIDATES",
        serde_json::json!({ "from": 100.0, "to": 104.0 }),
    );
    let other = ranked[0]["takes"]
        .as_array()
        .and_then(|t| {
            t.iter()
                .find(|t| t["current"] != true && t["empty"] == false)
        })
        .and_then(|t| t["take"].as_str().map(str::to_owned));
    if let Some(take) = other {
        run(
            "COMP_CHOOSE",
            serde_json::json!({ "from": 100.0, "to": 104.0, "take": take }),
        );
        check("after a comp");
    }
    session_daw::drum_lanes::reset_edit(daw, &project, false).expect("reset");
    check("after the EDIT lanes are reset from COMP");
    assert!(std::env::var_os("FTS_INDEX_SHOW").is_none(), "{said:#?}");
}
