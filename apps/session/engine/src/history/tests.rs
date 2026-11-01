//! The song's history: steps of the same name stay apart, each is gone
//! back to — after the song is reopened too — as a step undone like any
//! other, and a mark holds the song as it was, what no step recorded
//! included.

use std::path::Path;

use daw::standalone::Standalone;
use session_format::store::{MAIN, MomentKind, MomentRef};

use crate::project_file::{self, Undone};

/// Song `guid` seeded into `daw` with one track, `Kick`, at `volume`, and
/// saved to `file`: open from it, its one bridge.
fn song_in(daw: &Standalone, guid: &str, file: &Path, volume: f64) {
    daw.seed_project(daw_proto::ProjectInfo {
        guid: guid.into(),
        name: guid.into(),
        path: String::new(),
    });
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(async {
            let facade = daw::standalone::bootstrap::build_in_process_daw(daw.clone())
                .await
                .expect("a facade");
            let project = facade.daw.project(guid.to_owned()).await.expect("the song");
            project
                .tracks()
                .add_with_guid("{KICK}", "Kick", None)
                .await
                .expect("a track");
        });
    set_volume(daw, guid, volume);
    project_file::save(daw, guid, file, MAIN, "Save").expect("saved");
}

/// The kick's fader, in the engine.
fn set_volume(daw: &Standalone, guid: &str, volume: f64) {
    daw.write_project(guid, |p| {
        if let Some(t) = p.tracks.iter_mut().find(|t| t.name == "Kick") {
            t.volume = volume;
        }
    });
}

fn volume(daw: &Standalone, guid: &str) -> Option<f64> {
    daw.read_project(guid, |p| {
        p.tracks.iter().find(|t| t.name == "Kick").map(|t| t.volume)
    })
    .flatten()
}

/// An edit and its step, as the engine finishing it would record it.
fn step(daw: &Standalone, guid: &str, volume: f64) {
    set_volume(daw, guid, volume);
    project_file::record(guid, "Set volume", Some("TRACK_SET")).expect("recorded");
}

/// The song's steps, newest first: (number, label, action).
fn steps(guid: &str) -> Vec<(i64, String, Option<String>)> {
    project_file::history(guid, (0, 50))
        .expect("its history")
        .0
        .into_iter()
        .filter(|(m, _)| m.kind == MomentKind::Step)
        .map(|(m, _)| (m.seq, m.label, m.action))
        .collect()
}

/// Two steps named alike are two steps, each its own version: going back
/// to the older one brings back its fader, not the newer's.
#[test]
fn two_steps_of_the_same_name_stay_two() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let daw = Standalone::new();
    song_in(&daw, "alike", &dir.path().join("Song.session"), 0.5);
    step(&daw, "alike", 0.6);
    step(&daw, "alike", 0.7);

    let named: Vec<(i64, String, Option<String>)> = steps("alike")
        .into_iter()
        .filter(|(_, label, _)| label == "Set volume")
        .collect();
    assert_eq!(named.len(), 2, "two steps: {named:?}");
    assert!(
        named
            .iter()
            .all(|(_, _, a)| a.as_deref() == Some("TRACK_SET"))
    );

    let older = named[1].0;
    project_file::restore("alike", MomentRef::Seq(older)).expect("gone back");
    assert_eq!(volume(&daw, "alike"), Some(0.6));
}

/// The history is the file's: reopened, the song lists its steps and goes
/// back to one.
#[test]
fn a_step_is_gone_back_to_after_the_song_is_reopened() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let daw = Standalone::new();
    let file = dir.path().join("Song.session");
    song_in(&daw, "first-open", &file, 0.5);
    step(&daw, "first-open", 0.6);
    let wanted = steps("first-open")[0].0;
    step(&daw, "first-open", 0.8);

    // Reopened as another session would: the file as it is on disk.
    let copy = dir.path().join("Reopened.session");
    std::fs::copy(&file, &copy).expect("the file");
    let wal = |f: &Path| {
        f.with_file_name(format!(
            "{}-wal",
            f.file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
        ))
    };
    if wal(&file).exists() {
        std::fs::copy(wal(&file), wal(&copy)).expect("its log");
    }
    let reopened = project_file::load(&daw, "Reopened", &copy, MAIN).expect("reopened");
    assert_eq!(volume(&daw, &reopened), Some(0.8));
    assert!(
        steps(&reopened).iter().any(|(seq, _, _)| *seq == wanted),
        "its steps came with it"
    );

    project_file::restore(&reopened, MomentRef::Seq(wanted)).expect("gone back");
    assert_eq!(volume(&daw, &reopened), Some(0.6));
}

/// Going back is a step of its own: undone, the song is as it was before
/// it — nothing of the history was rewritten.
#[test]
fn going_back_is_a_step_undone_like_any_other() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let daw = Standalone::new();
    song_in(&daw, "undoable", &dir.path().join("Song.session"), 0.5);
    step(&daw, "undoable", 0.6);
    let wanted = steps("undoable")[0].0;
    step(&daw, "undoable", 0.9);

    project_file::restore("undoable", MomentRef::Seq(wanted)).expect("gone back");
    assert_eq!(volume(&daw, "undoable"), Some(0.6));
    assert_eq!(
        steps("undoable")[0].1,
        "Back to Set volume",
        "listed as a step of its own"
    );

    let undone = project_file::undo(&daw, "undoable").expect("undone");
    assert_eq!(undone, Undone::Step("Back to Set volume".to_owned()));
    assert_eq!(volume(&daw, "undoable"), Some(0.9));
}

/// A mark holds the song as it is when it is set, an edit no step has
/// recorded yet included; going back to it by name brings that back.
#[test]
fn a_mark_holds_the_song_as_it_was_and_is_gone_back_to_by_name() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let daw = Standalone::new();
    song_in(&daw, "marked", &dir.path().join("Song.session"), 0.5);
    // Edited, no step recorded.
    set_volume(&daw, "marked", 0.25);
    let stamp = "2026-10-31 14:05";
    let mark = crate::safety_net::mark_before("marked", "DRUMS_REQUANTIZE", stamp).expect("marked");
    assert_eq!(mark, "Before DRUMS_REQUANTIZE 2026-10-31 14:05");
    // The rewrite; then another in the same minute.
    step(&daw, "marked", 0.9);
    let second =
        crate::safety_net::mark_before("marked", "DRUMS_REQUANTIZE", stamp).expect("marked");
    assert_eq!(second, "Before DRUMS_REQUANTIZE 2026-10-31 14:05 2");

    let marks: Vec<String> = project_file::history("marked", (0, 50))
        .expect("its history")
        .0
        .into_iter()
        .filter(|(m, _)| m.kind == MomentKind::Mark)
        .map(|(m, _)| m.label)
        .collect();
    assert_eq!(marks, [second.clone(), mark.clone()]);

    project_file::restore("marked", MomentRef::Mark(mark)).expect("gone back");
    assert_eq!(volume(&daw, "marked"), Some(0.25));
    project_file::restore("marked", MomentRef::Mark(second)).expect("gone back");
    assert_eq!(volume(&daw, "marked"), Some(0.9));
}
