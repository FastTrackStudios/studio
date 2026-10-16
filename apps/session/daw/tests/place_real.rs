//! A snare hit on a real song's kit moved a beat later and back
//! (`HITS_PLACE`), on a copy of it: moved back, every hit's attack plays
//! on every mic as it did before, nothing more is drawn out, no hit plays
//! twice that did not, a second round trip changes nothing — and each
//! move is timed.
//!
//! ```sh
//! FTS_PERF_SESSION=/path/to/a/copy/of/Unbreakable.session [FTS_PLACE_AT=84.0] [FTS_PLACE_BY_MS=500] \
//!   cargo nextest run --cargo-profile release-fast -p session-daw --features signal \
//!   --test place_real --run-ignored all
//! ```

use std::path::PathBuf;

use daw::service::{ItemRef, Items as _, ProjectContext, TrackRef, Tracks as _};
use daw_proto::{StretchMarkers as _, TakeRef};

/// What a mic's EDIT lane plays around a time: each item's place, length,
/// where in its recording it starts, and whether it is drawn out.
type Lane = Vec<(String, Vec<(f64, f64, f64, bool)>)>;

fn edit_lanes(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (from, to): (f64, f64),
) -> Lane {
    use session_daw::drum_lanes::{EDIT, lane_named};
    daw.all(project.clone())
        .into_iter()
        .filter_map(|t| {
            let edit = lane_named(&t, EDIT)?;
            let mut items: Vec<(f64, f64, f64, bool)> = daw
                .get_items(project.clone(), TrackRef::Guid(t.guid.clone()))
                .into_iter()
                .filter(|i| i.fixed_lane.unwrap_or(0) == edit)
                .filter(|i| {
                    let at = i.position.as_seconds();
                    at + i.length.as_seconds() > from && at < to
                })
                .map(|i| {
                    let drawn = daw
                        .get_stretch_markers(
                            project.clone(),
                            ItemRef::Guid(i.guid.clone()),
                            TakeRef::Active,
                        )
                        .len()
                        >= 2;
                    (
                        i.position.as_seconds(),
                        i.length.as_seconds(),
                        i.start_offset.as_seconds(),
                        drawn,
                    )
                })
                .collect();
            items.sort_by(|a, b| a.0.total_cmp(&b.0));
            Some((t.name, items))
        })
        .collect()
}

/// Where a lane plays recording time `source` at `t`: the item there, its
/// offset into its recording, unstretched.
fn plays(lane: &[(f64, f64, f64, bool)], t: f64) -> Option<f64> {
    lane.iter()
        .rev()
        .find(|(at, len, ..)| *at <= t && t < at + len)
        .map(|(at, _, offset, _)| offset + (t - at))
}

#[test]
#[ignore = "reads a real session; set FTS_PERF_SESSION"]
fn a_snare_moved_and_moved_back_plays_as_before() {
    let Some(path) = std::env::var_os("FTS_PERF_SESSION").map(PathBuf::from) else {
        return;
    };
    let near: f64 = std::env::var("FTS_PLACE_AT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(84.0);
    let by_ms: f64 = std::env::var("FTS_PLACE_BY_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(500.0);
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
    session_daw::open::open_silent(&path).expect("opens");
    let (daw, project) = session_daw::actions::song().expect("the song");
    let hits = session_daw::hits::playback(daw, &project);
    let snare = hits
        .iter()
        .filter(|h| h.piece.to_lowercase().contains("snare") && h.time >= near)
        .min_by(|a, b| a.time.total_cmp(&b.time))
        .expect("a snare hit after FTS_PLACE_AT")
        .clone();
    let around = (snare.time - 2.0, snare.time + 2.0 + by_ms / 1_000.0);
    let before = edit_lanes(daw, &project, around);

    let doubled = |hits: &[session_daw::hits::Played]| -> std::collections::BTreeSet<String> {
        let mut seen = std::collections::HashMap::<&str, usize>::new();
        for h in hits {
            *seen.entry(h.id.as_str()).or_default() += 1;
        }
        seen.into_iter()
            .filter(|(_, n)| *n > 1)
            .map(|(id, _)| id.to_owned())
            .collect()
    };
    let doubled_before = doubled(&hits);
    // Where the hits say the snare plays, after each call: what a failing
    // call is reported with.
    let trail = std::cell::RefCell::new(Vec::<String>::new());
    let snare_now = |label: &str| {
        let at: Vec<f64> = session_daw::hits::playback(daw, &project)
            .iter()
            .filter(|h| h.id == snare.id)
            .map(|h| h.time)
            .collect();
        trail
            .borrow_mut()
            .push(format!("{label}: the snare plays at {at:?}"));
    };
    let place = |by: f64| {
        let said = session_daw::actions::list().run_json(
            "HITS_PLACE",
            serde_json::json!({ "id": snare.id, "by_ms": by, "snap": false })
                .to_string()
                .as_bytes(),
        );
        let said =
            said.unwrap_or_else(|e| panic!("HITS_PLACE {by} ms: {e}\n{:#?}", trail.borrow()));
        snare_now(&format!("moved {by} ms"));
        serde_json::from_slice::<serde_json::Value>(&said).unwrap_or_default()
    };
    snare_now("before");
    let round_trip = || {
        let there = place(by_ms);
        let moved = edit_lanes(daw, &project, around);
        let back = place(-by_ms);
        (there, moved, back, edit_lanes(daw, &project, around))
    };
    let (there, moved, back, after) = round_trip();
    let doubled_after = doubled(&session_daw::hits::playback(daw, &project));
    let (_, _, _, again) = round_trip();

    let drawn = |lanes: &Lane| -> usize {
        lanes
            .iter()
            .map(|(_, items)| items.iter().filter(|i| i.3).count())
            .sum()
    };
    // Moved back, every hit's attack plays on every mic as it did.
    let attacks: Vec<f64> = hits
        .iter()
        .map(|h| h.time)
        .filter(|t| *t > around.0 + 0.1 && *t < around.1 - 0.1)
        .collect();
    let mut differs = Vec::new();
    for ((name, was), (_, now)) in before.iter().zip(&after) {
        for &t in &attacks {
            let (a, b) = (plays(was, t + 0.003), plays(now, t + 0.003));
            let same = match (a, b) {
                (Some(a), Some(b)) => (a - b).abs() < 1e-4,
                (None, None) => true,
                _ => false,
            };
            if !same {
                differs.push(format!("{name}, the hit at {t:.3}: {a:?} then {b:?}"));
            }
        }
    }
    let timed: Vec<String> = architect_telemetry::timing::recent(64)
        .into_iter()
        .filter(|t| t.field("action.id") == Some("HITS_PLACE"))
        .map(|t| format!("{:.0} ms {:?}", t.total_ms, t.stages))
        .collect();
    let said = format!(
        "snare {} at {:.3} s by {by_ms} ms: drawn out before {}, moved {}, back {}\n\
         doubled before {doubled_before:?}, after {doubled_after:?}\n\
         there: {there}\nback: {back}\ntimed: {timed:#?}",
        snare.id,
        snare.time,
        drawn(&before),
        drawn(&moved),
        drawn(&after)
    );
    assert!(
        differs.is_empty(),
        "moved back plays as before: {differs:#?}\n{said}"
    );
    assert!(
        drawn(&after) <= drawn(&before),
        "nothing left drawn out: {said}"
    );
    assert!(
        doubled_after.is_subset(&doubled_before),
        "no hit plays twice that did not: {said}"
    );
    assert_eq!(after, again, "a second round trip changes nothing: {said}");
    assert!(std::env::var_os("FTS_PLACE_SHOW").is_none(), "{said}");
}
