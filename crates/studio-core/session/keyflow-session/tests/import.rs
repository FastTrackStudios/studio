//! A song charted before charts lived in sessions: a `.kf` beside a
//! chartless one-file `.session`. Opening the song imports the chart into
//! its session once, as one undo step; a session's own chart is never
//! replaced.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::missing_panics_doc
)]

use std::path::{Path, PathBuf};

use keyflow_session::import::{self, Found, Outcome};
use song_file::SongFile;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/import/Silent Night.kf"
);

/// A song's folder as Drive has it: `Silent Night.session` (made
/// chartless) with `Silent Night.kf` beside it.
fn song_folder() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Silent Night - for KING & COUNTRY");
    std::fs::create_dir_all(&folder).unwrap();
    let song = folder.join("Silent Night.session");
    SongFile::open(&song).unwrap().close().unwrap();
    std::fs::copy(FIXTURE, folder.join("Silent Night.kf")).unwrap();
    (dir, song)
}

/// Opens `song` as the engine does and imports what is beside it.
fn open_and_import(song: &Path) -> import::Beside {
    let file = SongFile::open(song).unwrap();
    let beside = import::import_beside(file.session().session(), song, None).unwrap();
    file.close().unwrap();
    beside
}

fn chart_in(song: &Path) -> String {
    let file = SongFile::open(song).unwrap();
    let chart = keyflow_session::written_chart(file.session().session());
    file.close().unwrap();
    chart
}

#[test]
fn opening_a_chartless_song_imports_the_kf_beside_it() {
    let (_dir, song) = song_folder();
    assert_eq!(
        chart_in(&song),
        "",
        "the fixture's session starts chartless"
    );

    let beside = open_and_import(&song);
    assert_eq!(beside.outcome, Some(Outcome::Imported));
    assert!(matches!(&beside.found, Found::One(p) if p.ends_with("Silent Night.kf")));

    let expected = std::fs::read_to_string(FIXTURE).unwrap();
    assert_eq!(
        chart_in(&song),
        expected,
        "the chart is in the file once reopened"
    );
}

#[test]
fn a_second_open_does_nothing() {
    let (_dir, song) = song_folder();
    assert_eq!(open_and_import(&song).outcome, Some(Outcome::Imported));
    assert_eq!(open_and_import(&song).outcome, Some(Outcome::Same));
    assert_eq!(open_and_import(&song).outcome, Some(Outcome::Same));
    let file = SongFile::open(&song).unwrap();
    let doc = file.session().session().doc().clone();
    file.close().unwrap();
    let imports = doc
        .export_json_updates(&Default::default(), &doc.oplog_vv())
        .unwrap()
        .changes
        .iter()
        .filter(|c| c.msg.as_deref() == Some("Import chart from Silent Night.kf"))
        .count();
    assert_eq!(imports, 1, "one import step, however many opens");
}

#[test]
fn a_session_chart_is_never_replaced() {
    let (_dir, song) = song_folder();
    let own = "Silent Night\n\nVerse 1: C C F C\n";
    {
        let file = SongFile::open(&song).unwrap();
        keyflow_session::write_chart(file.session().session(), own).unwrap();
        file.close().unwrap();
    }
    assert_eq!(open_and_import(&song).outcome, Some(Outcome::Kept));
    assert_eq!(chart_in(&song), own, "the session's own chart stays");
}

#[test]
fn the_import_is_one_named_undo_step() {
    let file_dir = tempfile::tempdir().unwrap();
    let song = file_dir.path().join("Song.session");
    let file = SongFile::open(&song).unwrap();
    let session = file.session().session();
    let doc = session.doc();
    let mut undo = loro::UndoManager::new(doc);
    let source = std::fs::read_to_string(FIXTURE).unwrap();

    let outcome = import::import_chart(session, "Silent Night.kf", &source).unwrap();
    assert_eq!(outcome, Outcome::Imported);
    let last = doc.oplog_frontiers().iter().next().unwrap();
    let change = doc.get_change(last).unwrap();
    assert_eq!(
        change.message.as_deref(),
        Some("Import chart from Silent Night.kf")
    );

    assert!(undo.can_undo());
    undo.undo().unwrap();
    assert_eq!(
        keyflow_session::written_chart(session),
        "",
        "one undo takes the import back"
    );
    assert!(!undo.can_undo(), "and it was one step");
    file.close().unwrap();
}

#[test]
fn a_kf_that_does_not_read_is_refused_and_the_session_left_alone() {
    let file_dir = tempfile::tempdir().unwrap();
    let song = file_dir.path().join("Song.session");
    let file = SongFile::open(&song).unwrap();
    let session = file.session().session();
    assert!(import::import_chart(session, "Empty.kf", "  \n").is_err());
    assert_eq!(keyflow_session::written_chart(session), "");
    file.close().unwrap();
}

#[test]
fn an_arrangement_s_chart_is_in_its_folder() {
    let (_dir, song) = song_folder();
    let folder = song.parent().unwrap();
    let arr = folder.join("Arrangements").join("Key of F");
    std::fs::create_dir_all(&arr).unwrap();
    std::fs::copy(FIXTURE, arr.join("Silent Night (Key of F).kf")).unwrap();
    std::fs::copy(FIXTURE, arr.join("notes.kf")).unwrap();
    assert!(matches!(
        import::kf_for(&song, Some("Key of F")),
        Found::One(p) if p.ends_with("Silent Night (Key of F).kf")
    ));
    assert_eq!(import::kf_for(&song, Some("Live")), Found::Nothing);
}

#[test]
fn several_kf_and_none_named_for_the_song_is_not_a_guess() {
    let dir = tempfile::tempdir().unwrap();
    let song = dir.path().join("Song.session");
    std::fs::write(dir.path().join("a.kf"), "Verse: C\n").unwrap();
    std::fs::write(dir.path().join("b.kf"), "Verse: G\n").unwrap();
    assert!(matches!(import::kf_for(&song, None), Found::Several(v) if v.len() == 2));
}
