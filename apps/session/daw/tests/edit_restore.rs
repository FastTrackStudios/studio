//! Backspace's two presses, end to end on the golden session: the kit
//! set up to be edited (COMP and EDIT lanes), a section quantized, one
//! hit placed by hand — then `EDIT_REVERT` with `keep_manual` takes the
//! automated edit off and leaves the hand placement, and without it takes
//! that off too: every hit where the drummer played it.
//!
//! One test, one process: the daw facade is bound to the first engine
//! opened.

use std::path::Path;
use std::time::{Duration, Instant};

use dynamic_template::golden_session::{perform, shape};
use session_daw::actions::{list, take_window, window_pending};
use session_daw::arrange_edit::{Editor, Effect, SlipCut, kit_items, seconds_of};
use session_daw::keys::Action;
use session_daw::studio::StudioSession;

/// Carry out what the Action List queued for the window — a slip or a
/// restore — on the session as the engine has it now.
fn be_the_window(path: &Path) {
    for action in take_window() {
        let session =
            StudioSession::read_current(Some(path), None).expect("the session reads back");
        let mut project = (*session.project.0).clone();
        let seconds = |us: i64| num_traits::ToPrimitive::to_f64(&us).unwrap_or(0.0) / 1_000_000.0;
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
                    .expect("the kit's folder");
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
                assert!(session_daw::engine::apply_now(&edit), "{edit:?} applies");
            }
        }
        assert!(session_daw::engine::apply_now(
            &session_daw::engine::Edit::Done(ticket)
        ));
    }
}

/// Run `id` with `args` off this thread, being the window meanwhile.
fn run(path: &Path, id: &'static str, args: serde_json::Value) -> serde_json::Value {
    let call = std::thread::spawn(move || list().run_json(id, args.to_string().as_bytes()));
    let began = Instant::now();
    while !call.is_finished() {
        assert!(began.elapsed() < Duration::from_secs(60), "{id} finishes");
        if window_pending() {
            be_the_window(path);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let said = call
        .join()
        .expect("it ran")
        .unwrap_or_else(|e| panic!("{id} was refused: {e}"));
    serde_json::from_slice(&said).unwrap_or(serde_json::Value::Null)
}

#[test]
fn backspace_takes_off_the_automated_edit_then_the_hand_placements() {
    // The recorder, as the app installs it: where each action's time went.
    {
        use tracing_subscriber::layer::SubscriberExt as _;
        let filter = tracing_subscriber::EnvFilter::new("info").add_directive(
            architect_telemetry::timing::DIRECTIVE
                .parse()
                .expect("parses"),
        );
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::registry()
                .with(filter)
                .with(architect_telemetry::timing::Recorder::default()),
        );
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../features/dynamic-template/fixtures/golden/template.rpp");
    dynamic_template::golden_session::write_media(path.parent().expect("the fixtures"))
        .expect("the golden media writes");
    session_daw::open::open_silent(&path).expect("the golden session opens");
    let song = shape::maximal();
    let bar = 240.0 / song.bpm;
    let section = song
        .sections
        .iter()
        .find(|s| s.start >= 8 && s.end > s.start.saturating_add(3))
        .expect("a section past the intro");
    let (from, to) = (f64::from(section.start) * bar, f64::from(section.end) * bar);
    for (on, piece, _) in perform::groove(&song) {
        let name = match piece {
            perform::Piece::Kick => "Kick",
            perform::Piece::Snare => "Snare",
            _ => continue,
        };
        if on < from - 2.0 * bar || on > to + 2.0 * bar {
            continue;
        }
        let played = perform::kit_played(on, piece, song.bpm);
        list()
            .run_json(
                "HITS_ADD",
                serde_json::json!({ "piece": name, "time": played, "snap": false })
                    .to_string()
                    .as_bytes(),
            )
            .expect("a hit placed");
    }
    run(&path, "DRUMS_EDIT_LANES", serde_json::json!({}));

    let (daw, project) = session_daw::actions::song().expect("the song is open");
    let recorded = session_daw::hits::hits(daw, &project, &[]);
    // A hit in a stretch the comp has no audio for plays nowhere: only
    // those that play are held up to where they were recorded.
    let playing: Vec<String> = recorded
        .iter()
        .filter(|h| h.now.is_some())
        .map(|h| h.id.clone())
        .collect();
    let inside = |t: f64| t >= from + 0.05 && t < to - 0.05;
    let as_recorded = |except: Option<&str>| {
        let now = session_daw::hits::hits(daw, &project, &[]);
        now.iter()
            .filter(|h| inside(h.time) && playing.contains(&h.id) && Some(h.id.as_str()) != except)
            .all(|h| h.now.is_some_and(|n| (n - h.time).abs() < 0.001))
    };

    let quantized = run(
        &path,
        "EDIT_QUANTIZE",
        serde_json::json!({ "from": from, "to": to, "grid": 16, "target": "Grid", "brief": true }),
    );
    assert!(!as_recorded(None), "the quantize moved hits: {quantized}");

    // A hit placed by hand, 20 ms from where the quantize put it.
    let hit = recorded
        .iter()
        .filter(|h| inside(h.time))
        .nth(4)
        .expect("a hit in the section");
    run(
        &path,
        "HITS_PLACE",
        serde_json::json!({ "id": hit.id, "by_ms": 20, "snap": false }),
    );
    let placed_at = session_daw::hits::playback(daw, &project)
        .into_iter()
        .find(|p| p.id == hit.id)
        .map(|p| p.time)
        .expect("it plays");

    // First press: the automated edit off, the hand placement kept.
    let first = run(
        &path,
        "EDIT_REVERT",
        serde_json::json!({ "from": from, "to": to, "hold": false, "keep_manual": true }),
    );
    assert_eq!(first["restored"], true, "{first}");
    assert_eq!(first["placed_back"], 1, "{first}");
    assert!(as_recorded(Some(&hit.id)), "the rest as recorded: {first}");
    let kept = session_daw::hits::playback(daw, &project)
        .into_iter()
        .find(|p| p.id == hit.id)
        .map(|p| p.time)
        .expect("it plays");
    assert!(
        (kept - placed_at).abs() < 0.001,
        "the hand placement kept: {placed_at:.4} → {kept:.4}"
    );

    // Second press: the hand placement off too.
    let second = run(
        &path,
        "EDIT_REVERT",
        serde_json::json!({ "from": from, "to": to, "hold": false }),
    );
    assert_eq!(second["restored"], true, "{second}");
    assert_eq!(second["forgotten"], 1, "{second}");
    assert!(as_recorded(None), "every hit as recorded: {second}");
    assert!(
        !session_daw::hits::placed(daw, &project).contains(&hit.id),
        "no longer a hand placement"
    );

    // Each edit's time, stage by stage, read back as a model reads it.
    let timed = run(
        &path,
        "PERF_RECENT",
        serde_json::json!({ "action": "EDIT_QUANTIZE", "limit": 1 }),
    );
    let stages: Vec<&str> = timed[0]["stages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s["key"].as_str())
        .collect();
    for stage in [
        "edit.read_ms",
        "edit.plan_ms",
        "edit.slip_ms",
        "edit.land_ms",
        "edit.check_ms",
    ] {
        assert!(stages.contains(&stage), "{stage} timed: {timed}");
    }
    let reverts = run(
        &path,
        "PERF_RECENT",
        serde_json::json!({ "action": "EDIT_REVERT" }),
    );
    assert_eq!(reverts.as_array().map(Vec::len), Some(2), "{reverts}");
}
