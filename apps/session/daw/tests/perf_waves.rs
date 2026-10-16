//! Waveforms on a real song, held to a budget: every audio item's at open,
//! read once a recording and cut from it; and after an edit, the new
//! slices' cut from what is already drawn — no read — matching what the
//! engine would have read.
//!
//! ```sh
//! FTS_PERF_SESSION=/path/to/a/copy/of/Unbreakable.session \
//!   cargo nextest run --cargo-profile release-fast -p session-daw --features signal \
//!   --test perf_waves --run-ignored all
//! ```

use std::path::PathBuf;

use architect_telemetry::timing::{self, Stages};
use daw::service::{ItemRef, ProjectContext, TakeRef};
use daw_proto::Peaks as _;
use session_daw::arrange_edit::{Editor, Effect, SlipCut, kit_items};
use session_daw::midi::Previews;
use session_daw::studio::StudioSession;

/// How long the slices an edit made may take to draw, ms. Cutting them is
/// about a millisecond; the rest is the engine's lock, held by the song's
/// bridge reading the whole song back after the edit (~80 ms on
/// Unbreakable) — to come down when it reads only what changed.
const AFTER_EDIT_MS: f64 = 120.0;

#[test]
#[ignore = "reads a real session; set FTS_PERF_SESSION"]
fn waveforms_on_a_real_song() {
    let Some(path) = std::env::var_os("FTS_PERF_SESSION").map(PathBuf::from) else {
        return;
    };
    {
        use tracing_subscriber::layer::SubscriberExt as _;
        let filter = timing::DIRECTIVES.iter().fold(
            tracing_subscriber::EnvFilter::new("warn,perf_waves=info"),
            |f, d| f.add_directive(d.parse().expect("parses")),
        );
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::registry()
                .with(filter)
                .with(timing::Recorder::default()),
        );
    }
    session_daw::open::open_silent(&path).expect("opens");
    let session = StudioSession::read_current(Some(&path), None).expect("reads back");
    let raw = (*session.planner.raw).clone();
    let audio: Vec<String> = raw
        .items
        .values()
        .flatten()
        .filter(|i| !raw.is_midi(&i.guid))
        .map(|i| i.guid.clone())
        .collect();

    let span = tracing::info_span!("perf.waves");
    let entered = span.enter();
    let mut stages = Stages::start();
    let previews = Previews::default();
    previews.fill_waves_blocking(audio.clone());
    stages.mark("perf.open_waves_ms");
    // An empty item (no recording) has nothing to draw.
    let (daw0, song0) = session_daw::actions::song().expect("the song");
    let undrawn: Vec<&String> = audio
        .iter()
        .filter(|g| previews.wave(g).is_none())
        .filter(|g| {
            use daw::service::Takes as _;
            daw0.get_active_take(song0.clone(), ItemRef::Guid((*g).clone()))
                .and_then(|t| t.source_file_path)
                .is_some()
        })
        .collect();
    let missing = undrawn.len();

    // A slip of a few hits, as the window makes it; its new slices drawn.
    let mut project = raw.clone();
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
    let mut effects = Vec::new();
    Editor::slip_items(
        &mut project,
        &items,
        &moves,
        ("Slip edit drums", cut),
        &mut effects,
    );
    for effect in effects {
        if let Effect::Send(edit) = effect {
            assert!(session_daw::engine::apply_now(&edit), "{edit:?}");
        }
    }
    let after = StudioSession::read_current(Some(&path), None).expect("reads back");
    let fresh: Vec<String> = after
        .planner
        .raw
        .items
        .values()
        .flatten()
        .filter(|i| !after.planner.raw.is_midi(&i.guid) && previews.wave(&i.guid).is_none())
        .map(|i| i.guid.clone())
        .collect();
    stages.mark("perf.edit_ms");
    previews.fill_waves_blocking(fresh.clone());
    stages.mark("perf.edit_waves_ms");
    drop(entered);
    drop(span);

    // A derived slice is what the engine reads for it.
    let song = ProjectContext::Project(session_daw::open::current_song().expect("current"));
    let worst = fresh
        .iter()
        .take(20)
        .filter_map(|guid| {
            let read = session_daw::midi::Wave::from_peaks(&daw.take_peaks(
                song.clone(),
                ItemRef::Guid(guid.clone()),
                TakeRef::Active,
                session_daw::midi::WAVE_BLOCK,
            ))?;
            let drawn = previews.wave(guid)?;
            Some(
                read.points
                    .iter()
                    .zip(&drawn.points)
                    .map(|(a, b)| (a.0 - b.0).abs().max((a.1 - b.1).abs()))
                    .fold(0.0_f32, f32::max),
            )
        })
        .fold(0.0_f32, f32::max);

    let timed = timing::recent(64)
        .into_iter()
        .find(|t| t.name == "perf.waves")
        .expect("timed");
    let all: Vec<String> = timed
        .stages
        .iter()
        .map(|(k, ms)| format!("{k} {ms:.1}"))
        .collect();
    let said = format!(
        "{} audio items ({} undrawn), {} new after the slip, worst difference {worst:.3}: {}",
        audio.len(),
        missing,
        fresh.len(),
        all.join(", ")
    );
    assert_eq!(
        missing, 0,
        "every waveform drawn at open: {said}\n{undrawn:#?}"
    );
    assert!(
        worst < 0.02,
        "a derived slice is what the engine reads: {said}"
    );
    assert!(
        timed.stage("perf.edit_waves_ms").unwrap_or(f64::MAX) <= AFTER_EDIT_MS,
        "an edit's slices drawn at once: {said}"
    );
    assert!(std::env::var_os("FTS_WAVES_SHOW").is_none(), "{said}");
}
