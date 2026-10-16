//! T on a real song's kit, on a copy of it: `COMP_CYCLE` over a stretch
//! comps every mic from its own recording of the next take — the mics'
//! lanes need not be numbered alike — leaves no mic's comp blank there,
//! and a second press on the same stretch steps from the ranking it kept.
//!
//! ```sh
//! FTS_PERF_SESSION=/path/to/a/copy/of/Unbreakable.session FTS_COMP_SPAN=83.995,85.6875 \
//!   cargo nextest run --cargo-profile release-fast -p session-daw --features signal \
//!   --test comp_cycle_real --run-ignored all
//! ```

use std::path::PathBuf;
use std::time::{Duration, Instant};

use daw::service::Items as _;
use session_daw::actions::{list, take_window, window_pending};
use session_daw::arrange_edit::{Editor, Effect};
use session_daw::keys::Action;
use session_daw::studio::StudioSession;

/// Be the window for the restore a comp queues.
fn be_the_window(path: &std::path::Path) {
    for action in take_window() {
        let Action::RestoreDrums {
            from_us,
            to_us,
            ticket,
        } = action
        else {
            continue;
        };
        let session = StudioSession::read_current(Some(path), None).expect("reads back");
        let mut project = (*session.project.0).clone();
        let seconds = |us: i64| num_traits::ToPrimitive::to_f64(&us).unwrap_or(0.0) / 1e6;
        let folder = project
            .tracks
            .iter()
            .find(|t| t.parent_guid.is_none() && t.name == "Drums")
            .map(|t| t.guid.clone())
            .expect("the kit");
        let mut effects = Vec::new();
        Editor::restore(
            &mut project,
            &folder,
            seconds(from_us),
            seconds(to_us),
            &mut effects,
        );
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

fn run(path: &std::path::Path, id: &'static str, args: serde_json::Value) -> serde_json::Value {
    let call = std::thread::spawn(move || list().run_json(id, args.to_string().as_bytes()));
    let began = Instant::now();
    while !call.is_finished() {
        assert!(began.elapsed() < Duration::from_secs(120), "{id} finishes");
        if window_pending() {
            be_the_window(path);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let said = call
        .join()
        .expect("ran")
        .unwrap_or_else(|e| panic!("{id}: {e}"));
    serde_json::from_slice(&said).unwrap_or_default()
}

#[test]
#[ignore = "reads a real session; set FTS_PERF_SESSION"]
fn t_comps_every_mic_from_its_own_lane() {
    let Some(path) = std::env::var_os("FTS_PERF_SESSION").map(PathBuf::from) else {
        return;
    };
    let (from, to) = std::env::var("FTS_COMP_SPAN")
        .ok()
        .and_then(|s| {
            let (a, b) = s.split_once(',')?;
            Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?))
        })
        .unwrap_or((83.995, 85.6875));
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
    // Every kit track with a COMP lane covers the stretch on it.
    let gaps = || -> Vec<String> {
        daw::service::Tracks::all(daw, project.clone())
            .into_iter()
            .filter_map(|t| {
                let comp = t
                    .lane_names
                    .iter()
                    .position(|n| n.trim().eq_ignore_ascii_case("COMP"))?;
                let comp = u32::try_from(comp).ok()?;
                let mut spans: Vec<(f64, f64)> = daw
                    .get_items(
                        project.clone(),
                        daw::service::TrackRef::Guid(t.guid.clone()),
                    )
                    .into_iter()
                    .filter(|i| i.fixed_lane.unwrap_or(0) == comp)
                    .map(|i| {
                        let a = i.position.as_seconds();
                        (a, a + i.length.as_seconds())
                    })
                    .filter(|(a, b)| *b > from && *a < to)
                    .collect();
                if spans.is_empty() {
                    return None;
                }
                spans.sort_by(|x, y| x.0.total_cmp(&y.0));
                let mut reach = spans[0].0;
                let mut gap = reach > from + 0.002;
                for (a, b) in spans {
                    gap |= a > reach + 0.002;
                    reach = reach.max(b);
                }
                gap |= reach < to - 0.002;
                gap.then(|| t.name.clone())
            })
            .collect()
    };
    assert!(gaps().is_empty(), "covered before: {:?}", gaps());
    let ranked = run(
        &path,
        "COMP_CANDIDATES",
        serde_json::json!({ "from": from, "to": to }),
    );
    let was = ranked[0]["takes"]
        .as_array()
        .and_then(|t| t.iter().find(|t| t["current"] == true))
        .map(|t| t["take"].clone());

    let first = run(
        &path,
        "COMP_CYCLE",
        serde_json::json!({ "from": from, "to": to, "by": 1 }),
    );
    let took = first[0]["take"].clone();
    assert_ne!(Some(&took), was.as_ref(), "a different take: {first}");
    assert!(gaps().is_empty(), "no mic blank after T: {:?}", gaps());
    let second = run(
        &path,
        "COMP_CYCLE",
        serde_json::json!({ "from": from, "to": to, "by": -1 }),
    );
    assert_eq!(
        Some(&second[0]["take"]),
        was.as_ref(),
        "Shift+T goes back: {second}"
    );
    assert!(
        gaps().is_empty(),
        "no mic blank after Shift+T: {:?}",
        gaps()
    );

    let presses: Vec<_> = architect_telemetry::timing::recent(64)
        .into_iter()
        .filter(|t| t.field("action.id") == Some("COMP_CYCLE"))
        .collect();
    assert_eq!(presses.len(), 2);
    let rank_ms = |t: &architect_telemetry::timing::Timed| t.stage("comp.rank_ms").unwrap_or(0.0);
    // Newest first: the second press reused the ranking the first made.
    assert!(
        rank_ms(&presses[0]) < 5.0,
        "the ranking kept: {:.1} ms then {:.1} ms",
        rank_ms(&presses[1]),
        rank_ms(&presses[0])
    );
    let shown: Vec<String> = presses
        .iter()
        .map(|t| format!("{:.0} ms {:?}", t.total_ms, t.stages))
        .collect();
    assert!(std::env::var_os("FTS_COMP_SHOW").is_none(), "{shown:#?}");
}
