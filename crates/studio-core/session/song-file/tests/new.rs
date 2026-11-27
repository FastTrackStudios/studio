//! A new song from nothing: one file, a minted id, the tempo given in the
//! engine's project and the session; a folder holding a song is refused
//! and nothing in it changes.

use std::path::Path;

use session_format::store::MAIN;
use song_file::{Error, Layout, SongFile, Tempo, new_song};

fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

#[test]
fn a_new_song_is_one_file_with_an_id_and_its_tempo() {
    let dir = tempfile::tempdir().unwrap();
    let song = dir.path().join("Praise - Elevation Worship");
    // Material not converted yet sits beside it and is left alone.
    std::fs::create_dir_all(song.join("Inbox")).unwrap();
    std::fs::write(song.join("Inbox/Praise.RPP"), "<REAPER_PROJECT\n>").unwrap();

    let tempo = Tempo {
        bpm: 127.0,
        beats_per_bar: 4,
        beat_unit: 4,
    };
    let made = new_song(&song, "Praise", Some(tempo)).unwrap();
    assert_eq!(made.file, song.join("Praise.session"));
    assert_eq!(names(&song), ["Inbox", "Praise.session"]);
    assert!(uuid::Uuid::parse_str(&made.id).is_ok());
    assert_eq!(made.session.tempo_points, 1);
    assert_eq!(
        Layout::of(&song).unwrap(),
        Layout::OneFile {
            file: made.file.clone()
        }
    );
    // The id is read from the file's bytes, as a setlist's reader does.
    assert_eq!(
        session_format::read::song_id(&std::fs::read(&made.file).unwrap()),
        Some(made.id.clone())
    );

    let opened = SongFile::open(&made.file).unwrap();
    assert_eq!(opened.id().unwrap().to_string(), made.id);
    let points = opened.session().session().tempo().points().to_vec();
    assert_eq!(points.len(), 1);
    assert!((points[0].bpm - 127.0).abs() < 1e-9);
    opened.with_store(|st| {
        let project = session_sync::SessionDoc::from_loro(st.load(MAIN).unwrap()).read();
        assert!(project.tracks.is_empty());
        assert_eq!(project.tempo.len(), 1);
        assert!((project.tempo[0].bpm - 127.0).abs() < 1e-9);
    });
    opened.close().unwrap();
}

#[test]
fn two_new_songs_get_two_ids() {
    let dir = tempfile::tempdir().unwrap();
    let a = new_song(&dir.path().join("a"), "A", None).unwrap();
    let b = new_song(&dir.path().join("b"), "B", None).unwrap();
    assert_ne!(a.id, b.id);
    assert_eq!(a.session.tempo_points, 0, "no tempo given: the default");
}

#[test]
fn a_folder_with_a_song_is_refused_and_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let song = dir.path().join("song");
    let made = new_song(&song, "Washed", None).unwrap();
    let before = std::fs::read(&made.file).unwrap();
    assert!(new_song(&song, "Washed", None).is_err());
    assert!(new_song(&song, "Another", None).is_err());
    assert_eq!(names(&song), ["Washed.session"]);
    assert_eq!(std::fs::read(&made.file).unwrap(), before);
}

#[test]
fn a_title_that_is_not_a_file_name_or_a_tempo_that_is_not_one_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    for title in ["", "a/b", ".hidden"] {
        assert!(matches!(
            new_song(dir.path(), title, None),
            Err(Error::Refused(_))
        ));
    }
    let zero = Tempo {
        bpm: 0.0,
        beats_per_bar: 4,
        beat_unit: 4,
    };
    assert!(matches!(
        new_song(dir.path(), "X", Some(zero)),
        Err(Error::Refused(_))
    ));
    assert!(names(dir.path()).is_empty());
}
