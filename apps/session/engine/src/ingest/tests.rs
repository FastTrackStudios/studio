use std::path::{Path, PathBuf};

use super::inputs::{self, Input, SongSource};
use super::{Media, Request, setlist_title, upload};

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
fn two_songs_that_would_be_one_in_the_library_are_caught() {
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
fn the_setlist_is_named_as_asked_else_for_the_project_else_the_one_show() {
    let show = [Input::Show(PathBuf::from("/x/Sr. Recital Showcase.als"))];
    let two = [
        Input::Song(PathBuf::from("/x/A/A.RPP")),
        Input::Song(PathBuf::from("/x/B/B.RPP")),
    ];
    let mut request = Request::new(Vec::new());
    assert_eq!(
        setlist_title(&request, &show).as_deref(),
        Some("Sr. Recital Showcase")
    );
    assert_eq!(setlist_title(&request, &two), None, "two songs name no set");
    request.project = Some("JT's Senior Recital".into());
    assert_eq!(
        setlist_title(&request, &two).as_deref(),
        Some("JT's Senior Recital")
    );
    request.setlist = Some("  Encore  ".into());
    assert_eq!(setlist_title(&request, &show).as_deref(), Some("Encore"));
}

#[test]
fn a_song_folder_is_read_as_what_travels_and_nothing_else() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    touch(&t.join("Song.session"), b"project");
    touch(&t.join("Song.session-wal"), b"");
    touch(&t.join(".fts-root.json"), b"{}");
    touch(&t.join(".fts-files/store/x"), b"x");
    touch(&t.join("Media/Bass.wav"), b"RIFF");
    touch(&t.join("Media/Proxies/Bass.ogg"), b"OggS");
    let rels = |media| -> Vec<String> {
        upload::scan(t, media)
            .unwrap()
            .into_iter()
            .map(|l| l.rel)
            .collect()
    };
    assert_eq!(
        rels(Media::Originals),
        ["Media/Bass.wav", "Media/Proxies/Bass.ogg", "Song.session"]
    );
    assert_eq!(
        rels(Media::ProxiesOnly),
        ["Media/Proxies/Bass.ogg", "Song.session"]
    );
}

#[test]
fn a_second_look_at_an_unchanged_folder_sends_nothing_and_hashes_nothing() {
    use session::library::publish::RemoteFile;
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    touch(&t.join("Song.session"), b"project");
    touch(&t.join("Media/Bass.wav"), b"RIFFdata");
    let locals = upload::scan(t, Media::Originals).unwrap();
    // The server has the bass whole (adopted after an rsync) and nothing else.
    let bass = upload::blake3_of(&t.join("Media/Bass.wav")).unwrap();
    let remote = vec![RemoteFile {
        path: "Media/Bass.wav".into(),
        size: 8,
        content: super::plan::whole_id(&bass, 8),
    }];
    let (sends, skipped, ledger) =
        upload::batch(locals.clone(), &remote, super::ledger::Ledger::default(), t).unwrap();
    assert_eq!(skipped, 1);
    assert_eq!(
        sends
            .iter()
            .map(|s| s.local.rel.as_str())
            .collect::<Vec<_>>(),
        ["Song.session"]
    );
    // Recorded as the server has it: next time the bass is not even hashed.
    let kept = &ledger.files["Media/Bass.wav"];
    assert!(kept.describes(&locals[0]) && kept.remote == remote[0].content);
    std::fs::remove_file(t.join("Media/Bass.wav")).unwrap();
    let still: Vec<_> = locals
        .into_iter()
        .filter(|l| l.rel == "Media/Bass.wav")
        .collect();
    let (sends, skipped, _) = upload::batch(still, &remote, ledger, t).unwrap();
    assert_eq!(
        (sends.len(), skipped),
        (0, 1),
        "vouched for without reading it"
    );
}

#[test]
fn a_second_ingest_is_refused_while_one_runs() {
    use super::actions::{IngestRow, claim};
    let slot = std::sync::Mutex::new(None);
    assert_eq!(claim(&slot), Ok(None));
    assert!(claim(&slot).is_err(), "two ingests at once");
    let done = IngestRow {
        state: "done".into(),
        ..IngestRow::default()
    };
    let slot = std::sync::Mutex::new(Some(done.clone()));
    assert_eq!(claim(&slot), Ok(Some(done)), "a finished one is no bar");
}

#[test]
fn what_no_worker_took_is_what_did_not_go() {
    let queue = [(0, 0), (0, 1), (1, 0)];
    assert_eq!(upload::unsent(&queue, 0), &queue[..], "nobody connected");
    assert_eq!(upload::unsent(&queue, 1), &queue[1..]);
    // Every worker overshoots `next` once on its way out.
    assert!(upload::unsent(&queue, 7).is_empty());
}

#[test]
fn an_upload_is_resumed_only_for_the_file_it_began_from() {
    use super::plan::{Kept, Local, Why};
    let id = "6f1c1a52-7b3e-4f7e-9d0a-0c4b2a1e9f10";
    let send = upload::Send {
        local: Local {
            rel: "Media/Kick.wav".into(),
            size: 10,
            modified: 5,
        },
        hash: Some("aa".into()),
        why: Why::New,
    };
    let kept = Kept {
        size: 10,
        modified: 5,
        blake3: Some("aa".into()),
        remote: None,
        pending: Some(id.into()),
    };
    let resumed = |k: &Kept| upload::resumable(Some(k), &send).map(|u| u.to_string());
    assert_eq!(resumed(&kept), Some(id.to_owned()));
    let touched = Kept {
        modified: 6,
        ..kept.clone()
    };
    assert_eq!(resumed(&touched), None, "changed since, same size");
    let other_bytes = Kept {
        blake3: Some("bb".into()),
        ..kept.clone()
    };
    assert_eq!(resumed(&other_bytes), None, "same size and time, other bytes");
    assert_eq!(upload::resumable(None, &send).map(|u| u.to_string()), None);
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
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
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
