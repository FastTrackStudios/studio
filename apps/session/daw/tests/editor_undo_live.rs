//! The Editor's note edits are Session's undo: a write-back of the take is
//! one step in the song's doc, named for what it is, and undoing it puts
//! the notes back.
//!
//! One test, one process: the daw facade is bound to the first engine
//! opened.

use daw::service::midi::{MidiNoteCreate, MidiTakeContent};
use daw::service::{Midi, ProjectContext, TrackRef, Tracks};
use daw_proto::item::ItemRef;

const EMPTY: &str = "<REAPER_PROJECT 0.1 \"7.0/test\" 0\n  TEMPO 120 4 4\n>\n";

/// The Editor's ticks to a quarter note.
const TICKS: f64 = 960.0;

fn notes(velocity: u8) -> MidiTakeContent {
    MidiTakeContent {
        notes: (0..4)
            .map(|i| MidiNoteCreate::new(60, velocity, f64::from(i) * TICKS, TICKS / 2.0))
            .collect(),
        ..MidiTakeContent::default()
    }
}

#[test]
fn an_editor_write_back_is_one_step_in_the_songs_doc_and_undoes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let rpp = dir.path().join("Notes.RPP");
    std::fs::write(&rpp, EMPTY).expect("write project");
    let opened = session_daw::open::open_silent(&rpp).expect("open");
    let _runtime = session_daw::open::runtime()
        .expect("engine runtime")
        .enter();
    let saved = rpp.with_extension("session");
    session_daw::session_file::save_session(&opened.daw, &opened.project_guid, &saved)
        .expect("save");
    let song = ProjectContext::Project(opened.project_guid.clone());
    let steps = || -> Vec<String> {
        session_format::store::Store::open(&saved)
            .and_then(|store| store.steps(session_format::store::MAIN))
            .map(|steps| steps.into_iter().map(|s| s.label).collect())
            .unwrap_or_default()
    };
    let wait_for = |label: &str| {
        let began = std::time::Instant::now();
        while !steps().iter().any(|l| l == label) && began.elapsed().as_secs() < 20 {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            steps().iter().any(|l| l == label),
            "no {label:?} step: {:?}",
            steps()
        );
    };

    // A part to edit, as a step of its own.
    daw::service::Projects::begin_undo_block(&opened.daw, song.clone(), "Add Keys");
    let track = Tracks::add(&opened.daw, song.clone(), "Keys", None).expect("a track");
    let at = Midi::create_midi_item(&opened.daw, song.clone(), TrackRef::Guid(track), 0.0, 2.0)
        .expect("an item");
    daw::service::Projects::end_undo_block(&opened.daw, song.clone(), "Add Keys", None);
    let ItemRef::Guid(item) = at.item else {
        panic!("an item guid")
    };
    wait_for("Add Keys");

    // Two write-backs, as the Editor makes them: each its own step.
    assert!(session_daw::expression::save_take(&item, notes(100)));
    let velocities = || {
        session_daw::expression::load_take(&item, 120.0, 2.0)
            .expect("the take reads back")
            .notes
            .iter()
            .map(|n| n.velocity)
            .collect::<Vec<_>>()
    };
    let label = session_daw::expression::EDIT_NOTES;
    let began = std::time::Instant::now();
    while steps().iter().filter(|l| *l == label).count() < 1 && began.elapsed().as_secs() < 20 {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(session_daw::expression::save_take(&item, notes(40)));
    let began = std::time::Instant::now();
    while steps().iter().filter(|l| *l == label).count() < 2 && began.elapsed().as_secs() < 20 {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(
        steps().iter().filter(|l| *l == label).count(),
        2,
        "each write-back is a step of its own: {:?}",
        steps()
    );
    assert_eq!(velocities(), vec![40; 4]);

    // Session's undo — the Editor's Undo — takes the last edit back.
    let guid = opened.project_guid.clone();
    assert_eq!(
        session_daw::project_file::undo(&opened.daw, &guid),
        Ok(session_daw::project_file::Undone::Step(label.to_owned()))
    );
    assert_eq!(
        velocities(),
        vec![100; 4],
        "the notes as the edit before left them"
    );
    assert_eq!(
        session_daw::project_file::redo(&opened.daw, &guid),
        Ok(session_daw::project_file::Undone::Step(label.to_owned()))
    );
    assert_eq!(velocities(), vec![40; 4]);
}
