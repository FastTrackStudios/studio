//! The checkpoint before a drum rewrite: one per action as the user ran
//! it, only when it reaches far, uniquely named, refused when it cannot be
//! made — and a real branch of the song's file, holding the song as it
//! was.

use std::cell::Cell;

use daw::standalone::Standalone;
use session::sync::SessionDoc;
use session_format::store::MAIN;

use super::{BIG_STRETCH, Reach, guarded, name_for};

#[test]
fn a_name_already_taken_gets_the_next_number() {
    let stamp = "2026-10-31 14:05";
    assert_eq!(
        name_for("DRUMS_REQUANTIZE", stamp, &[]),
        "Before DRUMS_REQUANTIZE 2026-10-31 14:05"
    );
    let taken = vec![
        "Before DRUMS_REQUANTIZE 2026-10-31 14:05".to_owned(),
        "Before DRUMS_REQUANTIZE 2026-10-31 14:05 2".to_owned(),
    ];
    assert_eq!(
        name_for("DRUMS_REQUANTIZE", stamp, &taken),
        "Before DRUMS_REQUANTIZE 2026-10-31 14:05 3"
    );
}

#[test]
fn only_the_whole_song_or_a_big_stretch_is_far() {
    let cases = [
        (Reach::Song, true),
        (Reach::Nothing, false),
        (Reach::of_spans([(10.0, 10.0 + BIG_STRETCH)]), true),
        (Reach::of_spans([(0.0, 10.0), (50.0, 60.0)]), false),
        (Reach::of_spans([(0.0, 20.0), (50.0, 70.0)]), true),
        (Reach::of_spans([]), false),
    ];
    for (reach, far) in cases {
        assert_eq!(reach.is_big(), far, "{reach:?}");
    }
}

#[test]
fn an_action_run_by_another_runs_under_its_checkpoint() {
    let forks = Cell::new(0_u32);
    let fork = || {
        forks.set(forks.get() + 1);
        Ok(format!("Before X {}", forks.get()))
    };
    {
        let outer = guarded(Reach::Song, fork).expect("forked");
        let inner = guarded(Reach::Song, fork).expect("under the outer one");
        assert_eq!(inner.name(), outer.name());
        assert_eq!(outer.name().as_deref(), Some("Before X 1"));
    }
    // The outer one done: the next action forks its own.
    let next = guarded(Reach::Song, fork).expect("forked");
    assert_eq!(next.name().as_deref(), Some("Before X 2"));
    assert_eq!(forks.get(), 2);
}

#[test]
fn a_small_reach_forks_nothing_and_a_failed_fork_refuses() {
    let small =
        guarded(Reach::Seconds(4.0), || Err("must not fork".to_owned())).expect("nothing to fork");
    assert_eq!(small.name(), None);
    let refused = guarded(Reach::Song, || Err("the file refused".to_owned()));
    assert_eq!(
        refused.map(|n| n.name()),
        Err("the file refused".to_owned())
    );
    // A refusal holds nothing: the next action forks its own.
    let next = guarded(Reach::Song, || Ok("Before Y".to_owned())).expect("forked");
    assert_eq!(next.name().as_deref(), Some("Before Y"));
}

/// Song `guid` seeded into `daw` with one track, `Kick`, at `volume`.
fn seed_song(daw: &Standalone, guid: &str, volume: f64) {
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
}

fn set_volume(daw: &Standalone, guid: &str, volume: f64) {
    daw.write_project(guid, |p| {
        if let Some(t) = p.tracks.iter_mut().find(|t| t.name == "Kick") {
            t.volume = volume;
        }
    });
}

fn kick_volume(doc: session::sync::loro::LoroDoc) -> Option<f64> {
    SessionDoc::from_loro(doc)
        .read()
        .tracks
        .iter()
        .find(|t| t.name == "Kick")
        .map(|t| t.volume)
}

/// The checkpoint is a branch of the song's own file, listed with its
/// alternatives, holding the song as it was before the rewrite — with
/// what was unsaved — and a second one in the same minute is another.
#[test]
fn a_checkpoint_is_a_branch_holding_the_song_as_it_was() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let daw = Standalone::new();
    seed_song(&daw, "net-song", 0.5);
    let file = dir.path().join("Song.session");
    crate::project_file::save(&daw, "net-song", &file, MAIN, "Save").expect("saved");
    // Changed and not saved: the checkpoint holds it all the same.
    set_volume(&daw, "net-song", 0.25);

    let stamp = "2026-10-31 14:05";
    let first = super::fork_before(&daw, "net-song", "DRUMS_REQUANTIZE", stamp).expect("forked");
    assert_eq!(first, "Before DRUMS_REQUANTIZE 2026-10-31 14:05");
    // The rewrite, then another in the same minute.
    set_volume(&daw, "net-song", 0.9);
    let second = super::fork_before(&daw, "net-song", "DRUMS_REQUANTIZE", stamp).expect("forked");
    assert_eq!(second, "Before DRUMS_REQUANTIZE 2026-10-31 14:05 2");

    let listed: Vec<String> = crate::projects::alternatives(&dir.path().join("Song"))
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert!(
        listed.contains(&first) && listed.contains(&second),
        "listed with the song's versions: {listed:?}"
    );
    let held =
        |name: &str| kick_volume(crate::project_file::doc_of(&file.join(name)).expect("it opens"));
    assert_eq!(held(&first), Some(0.25), "the song as it was before");
    assert_eq!(held(&second), Some(0.9), "the song before the second");
}

#[test]
fn a_song_not_open_from_its_file_is_refused() {
    let daw = Standalone::new();
    seed_song(&daw, "fileless-song", 0.5);
    let refused = super::fork_before(&daw, "fileless-song", "QUANTIZE", "2026-10-31 14:05");
    assert!(
        refused
            .as_ref()
            .is_err_and(|why| why.contains("project file")),
        "{refused:?}"
    );
}
