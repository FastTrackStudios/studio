//! What a drum slip costs the window on a real song, stage by stage, held
//! to a budget.
//!
//! ```sh
//! FTS_PERF_SESSION=/path/to/a/copy/of/Unbreakable.session \
//!   cargo nextest run --cargo-profile release-fast -p session-daw --features signal \
//!   --test it perf_slip:: --run-ignored all
//! ```
//!
//! Opens the session (a copy: nothing is saved), slips three hits the way
//! the window does, and times what the window then runs on its own thread
//! — the slip, the copy `settle` makes for the planner, the row plan —
//! through the timing recorder the app installs. A stage over its budget
//! fails with every stage's time in the message.

use std::path::PathBuf;

use architect_telemetry::timing::{self, Stages};
use session_daw::arrange_edit::{Editor, SlipCut, kit_items};
use session_daw::studio::StudioSession;

/// Each stage's budget on an ordinary machine, ms: the window must not
/// freeze for longer than this after a slip.
const BUDGETS: [(&str, f64); 3] = [
    ("perf.slip_ms", 200.0),
    ("perf.settle_copy_ms", 200.0),
    ("perf.replan_ms", 1_000.0),
];

#[test]
#[ignore = "reads a real session; set FTS_PERF_SESSION"]
fn a_slip_on_a_real_song() {
    let Some(path) = std::env::var_os("FTS_PERF_SESSION").map(PathBuf::from) else {
        return;
    };
    {
        use tracing_subscriber::layer::SubscriberExt as _;
        // Its own span, and the stage marks; the session's chatter not.
        let filter = tracing_subscriber::EnvFilter::new("warn,perf_slip=info")
            .add_directive(timing::DIRECTIVE.parse().expect("parses"));
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::registry()
                .with(filter)
                .with(timing::Recorder::default()),
        );
    }
    let span = tracing::info_span!("perf.slip");
    let in_span = span.enter();
    let mut stages = Stages::start();
    session_daw::open::open_silent(&path).expect("the session opens");
    stages.mark("perf.open_ms");
    let session = StudioSession::read_current(Some(&path), None).expect("it reads back");
    stages.mark("perf.read_back_ms");
    let raw = session.planner.raw.clone();
    let _ = session.planner.plan(&raw);
    stages.mark("perf.plan_ms");

    let mut project = (*raw).clone();
    let items = kit_items(&project, &[]);
    let (daw, song) = session_daw::actions::song().expect("the song");
    let hits = session_daw::hits::playback(daw, &song);
    let mid = hits.len() / 2;
    let moves: Vec<(f64, f64)> = hits[mid..mid + 3].iter().map(|h| (h.time, 0.010)).collect();
    let cut = SlipCut {
        pre_roll: 0.005,
        crossfade: 0.005,
        stretch_over: None,
        attacks: hits.iter().map(|h| h.time).collect(),
    };
    stages.mark("perf.hits_ms");
    let mut effects = Vec::new();
    Editor::slip_items(
        &mut project,
        &items,
        &moves,
        ("Slip edit drums", cut),
        &mut effects,
    );
    stages.mark("perf.slip_ms");
    let mut copy = (*raw).clone();
    copy.items.clone_from(&project.items);
    stages.mark("perf.settle_copy_ms");
    let _ = session.planner.plan(&copy);
    stages.mark("perf.replan_ms");
    drop(in_span);
    drop(span);

    let timed = timing::recent(64)
        .into_iter()
        .find(|t| t.name == "perf.slip")
        .expect("the slip timed");
    let all: Vec<String> = timed
        .stages
        .iter()
        .map(|(k, ms)| format!("{k} {ms:.1}"))
        .collect();
    for (stage, budget) in BUDGETS {
        let ms = timed.stage(stage).expect("timed");
        assert!(
            ms <= budget,
            "{stage} took {ms:.1} ms, over its {budget} ms budget ({} kit takes, {} edits): {}",
            items.len(),
            effects.len(),
            all.join(", ")
        );
    }
}
