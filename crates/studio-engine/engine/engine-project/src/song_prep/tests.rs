use std::path::{Path, PathBuf};

use super::inputs::{self, Input, SongSource};

fn touch(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn each_kind_of_input_is_read_as_what_it_is() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    touch(&t.join("Show/Recital.als"), b"");
    touch(&t.join("Album/01 Intro/Intro.RPP"), b"");
    touch(&t.join("Album/02 Empty Focus/Empty Focus.RPP"), b"");
    touch(&t.join("Album/notes/readme.txt"), b"");
    touch(&t.join("Drive To LA/Drive To LA.session"), b"");
    touch(&t.join("Nothing/readme.txt"), b"");

    let read = |p: &str| inputs::classify(&t.join(p)).unwrap();
    assert_eq!(read("Show"), Input::Show(t.join("Show/Recital.als")));
    assert_eq!(
        read("Show/Recital.als"),
        Input::Show(t.join("Show/Recital.als"))
    );
    assert_eq!(read("Album"), Input::Folder(t.join("Album")));
    assert_eq!(
        read("Drive To LA"),
        Input::Song(t.join("Drive To LA/Drive To LA.session"))
    );
    assert_eq!(
        read("Drive To LA/Drive To LA.session"),
        Input::Song(t.join("Drive To LA/Drive To LA.session"))
    );
    assert!(inputs::classify(&t.join("Nothing")).is_err());
    assert!(inputs::classify(&t.join("Nothing/readme.txt")).is_err());
    assert!(inputs::classify(&t.join("not there")).is_err());

    let album: Vec<(String, String)> = inputs::songs_of(&read("Album"))
        .unwrap()
        .into_iter()
        .map(|s| (s.title, s.slug))
        .collect();
    assert_eq!(
        album,
        [
            ("Intro".to_owned(), "intro".to_owned()),
            ("Empty Focus".to_owned(), "empty-focus".to_owned()),
        ],
        "the album's songs in folder order, their track numbers dropped"
    );
}

#[test]
fn a_song_is_named_by_its_project_file() {
    assert_eq!(
        inputs::identity("06 He's Been Good"),
        ("He's Been Good".to_owned(), "hes-been-good".to_owned())
    );
    assert_eq!(
        inputs::identity("Drive To LA"),
        ("Drive To LA".to_owned(), "drive-to-la".to_owned())
    );
}

#[test]
fn two_songs_that_would_be_one_by_their_slug_are_caught() {
    let song = |title: &str| SongSource {
        title: title.into(),
        slug: inputs::identity(title).1,
        file: PathBuf::new(),
        folder: PathBuf::new(),
    };
    assert_eq!(
        inputs::clash(&[song("Fire"), song("Hey"), song("FIRE!")]),
        Some(("Fire", "FIRE!"))
    );
    assert_eq!(inputs::clash(&[song("Fire"), song("Hey")]), None);
}

#[test]
fn a_reference_made_before_the_log_is_folded_stays_current() {
    use super::prepare::{is_current, stamp_current};
    let tmp = tempfile::tempdir().unwrap();
    let made = [tmp.path().join("Reference.ogg")];
    let song = tmp.path().join("Song.session");
    touch(&made[0], b"ogg");
    touch(&song, b"song");
    // The fold writes the song after the bounce.
    let later = web_time::SystemTime::now() + std::time::Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(&song)
        .unwrap()
        .set_modified(later)
        .unwrap();
    assert!(!is_current(&made, &song));
    stamp_current(&made, &song).unwrap();
    assert!(is_current(&made, &song));
}
