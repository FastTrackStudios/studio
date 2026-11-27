//! One file format for a session, whoever saves it.

use loro::ExportMode;
use session::doc::SessionDoc;
use session::file::SessionFile;
use session::{ExtensionId, Time};

#[test]
fn a_session_file_keeps_what_was_saved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Drive To LA.session");
    let id = {
        let file = SessionFile::create(&path).unwrap();
        file.session()
            .set_bar_one(Some(Time::Seconds(6.0)))
            .unwrap();
        file.session()
            .ext(&ExtensionId::from("keyflow"))
            .unwrap()
            .set_text("chart", "Verse: C G Am F\n")
            .unwrap();
        file.save().unwrap();
        file.id()
    };
    let file = SessionFile::open(&path).unwrap();
    assert_eq!(file.id(), id);
    assert_eq!(file.session().bar_one(), Some(Time::Seconds(6.0)));
    let keyflow = file.session().read_ext(&ExtensionId::from("keyflow"));
    assert_eq!(keyflow.text("chart"), Some("Verse: C G Am F\n"));
}

/// A session saved as a bare Loro snapshot (the DAW engine's file before
/// the format) opens as a session file, brought into the format in place.
#[test]
fn an_engines_bare_snapshot_opens_as_a_session_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("drive-to-la.session");
    let engine = SessionDoc::new();
    engine
        .ext(&ExtensionId::from("daw"))
        .unwrap()
        .set_text("notes", "38 tracks")
        .unwrap();
    engine.doc().commit();
    std::fs::write(&path, engine.doc().export(ExportMode::Snapshot).unwrap()).unwrap();

    // Opened as a document file it is not one.
    assert!(SessionFile::open(&path).is_err());
    let file = SessionFile::open_or_import(&path).unwrap();
    let daw = file.session().read_ext(&ExtensionId::from("daw"));
    assert_eq!(daw.text("notes"), Some("38 tracks"));
    // The snapshot is kept beside it, and the file is a session file now.
    let mut kept = path.clone().into_os_string();
    kept.push(".snapshot");
    assert!(std::path::Path::new(&kept).exists());
    drop(file);
    let again = SessionFile::open(&path).unwrap();
    assert_eq!(
        again
            .session()
            .read_ext(&ExtensionId::from("daw"))
            .text("notes"),
        Some("38 tracks")
    );
}

/// A file holding parts by id, in memory: what the song's one file is to
/// its session.
#[derive(Default)]
struct Parts(std::sync::Mutex<std::collections::HashMap<uuid::Uuid, loro::LoroDoc>>);

impl session::file::PartStore for Parts {
    fn load(&self, id: uuid::Uuid) -> Result<Option<Vec<u8>>, String> {
        let parts = self.0.lock().unwrap();
        parts
            .get(&id)
            .map(|doc| doc.export(ExportMode::Snapshot).map_err(|e| e.to_string()))
            .transpose()
    }

    fn append(&self, id: uuid::Uuid, update: &[u8]) -> Result<(), String> {
        let imported = self.0.lock().unwrap().entry(id).or_default().import(update);
        imported.map(|_| ()).map_err(|e| e.to_string())
    }

    fn compact(&self, id: uuid::Uuid, snapshot: &[u8]) -> Result<(), String> {
        self.append(id, snapshot)
    }
}

/// A session kept as a part of another file (the song's one file) is the
/// same session: opened over that file, edited, saved, and opened again
/// with everything it held.
#[test]
fn a_session_kept_in_another_file() {
    use std::sync::Arc;
    let parts = Arc::new(Parts::default());
    let id = uuid::Uuid::new_v4();
    {
        let file = SessionFile::over_part(id, parts.clone()).unwrap();
        file.session()
            .set_bar_one(Some(Time::Seconds(4.0)))
            .unwrap();
        file.session()
            .ext(&ExtensionId::from("ignition"))
            .unwrap()
            .set_text("notes", "Lights down at the bridge")
            .unwrap();
        file.session().doc().commit();
        file.save().unwrap();
    }
    let file = SessionFile::over_part(id, parts).unwrap();
    assert_eq!(file.id(), id);
    assert_eq!(file.session().bar_one(), Some(Time::Seconds(4.0)));
    let ignition = file.session().read_ext(&ExtensionId::from("ignition"));
    assert_eq!(ignition.text("notes"), Some("Lights down at the bridge"));
}

/// Nothing waits for a save: every edit is in the file once the session
/// is let go, and an edit made while the part's own lock is held (the song
/// file's store, busy with something else) does not wait on it.
#[test]
fn every_edit_is_kept_without_a_save() {
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Unsaved.session");
    {
        let file = SessionFile::create(&path).unwrap();
        file.session()
            .ext(&ExtensionId::from("keyflow"))
            .unwrap()
            .set_text("chart", "Intro: C\n")
            .unwrap();
        file.session().doc().commit();
        file.save().unwrap();
        file.session()
            .ext(&ExtensionId::from("keyflow"))
            .unwrap()
            .set_text("chart", "Intro: C\nVerse: G\n")
            .unwrap();
        file.session().doc().commit();
    }
    let file = SessionFile::open(&path).unwrap();
    let keyflow = file.session().read_ext(&ExtensionId::from("keyflow"));
    assert_eq!(keyflow.text("chart"), Some("Intro: C\nVerse: G\n"));

    let parts = Arc::new(Parts::default());
    let id = uuid::Uuid::new_v4();
    let file = SessionFile::over_part(id, parts.clone()).unwrap();
    {
        let _busy = parts.0.lock().unwrap();
        file.session()
            .set_bar_one(Some(Time::Seconds(1.0)))
            .unwrap();
        file.session().doc().commit();
    }
    drop(file);
    let file = SessionFile::over_part(id, parts).unwrap();
    assert_eq!(file.session().bar_one(), Some(Time::Seconds(1.0)));
}

/// A file that is not there is refused, not made and taken for an empty
/// session.
#[test]
fn a_missing_file_is_refused_not_made() {
    let dir = tempfile::tempdir().unwrap();
    assert!(SessionFile::open(&dir.path().join("nowhere.session")).is_err());
    assert!(!dir.path().join("nowhere.session").exists());
}

#[test]
fn a_files_header_says_what_it_is() {
    use session::file::{Sniffed, sniff};
    assert_eq!(sniff(b"ARCHDOC\0\x01\0\0\0"), Sniffed::DocFile);
    assert_eq!(sniff(b"SQLite format 3\0"), Sniffed::Store);
    assert_eq!(sniff(b"Drive To LA"), Sniffed::Other);
}
