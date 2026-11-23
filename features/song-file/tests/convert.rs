//! The old pair folded into one file: everything it held comes through
//! (the id, the timing, the chart, the cues, the patch changes, the
//! engine's project with its blobs, history and branches); the old files
//! go out of the way, never deleted; a converted folder is left alone; and
//! the one file opens wherever it is copied to.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fts_session::file::SessionFile;
use fts_session::time::{Position, TempoPoint, TimePosition, TimeSignature};
use fts_session::{
    ExtensionId, Item, ItemId, Lane, LaneId, MarkId, Marker, Region, TempoMap, Time, Value,
};
use loro::{ExportMode, LoroDoc};
use session_format::store::{MAIN, MomentKind, SESSION, Store};
use song_file::{Layout, Options, Outcome, SongFile, convert};

const TITLE: &str = "Drive To LA";

fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

/// The engine's store as the engine leaves it: an update on `main`, a
/// blob, a step of history, an arrangement forked — its log not folded in.
fn old_project(path: &Path) -> serde_like::Project {
    let store = Store::open(path).unwrap();
    let doc = LoroDoc::new();
    let before = doc.oplog_vv();
    doc.get_map("tracks").insert("Bass", 0.8).unwrap();
    doc.commit();
    store
        .append_update(
            MAIN,
            "Add Bass",
            &doc.export(ExportMode::updates(&before)).unwrap(),
        )
        .unwrap();
    store
        .note(
            MAIN,
            (MomentKind::Step, "Add Bass", Some("TRACK_ADD")),
            &doc.state_frontiers(),
        )
        .unwrap();
    let blob = store.put_blob("fx", b"a compressor's state").unwrap();
    store.fork("Acoustic", MAIN, &doc).unwrap();
    serde_like::Project {
        main: doc.get_deep_value(),
        blob,
    }
}

mod serde_like {
    pub struct Project {
        pub main: loro::LoroValue,
        pub blob: String,
    }
}

/// The session document as Studio and Keyflow leave it.
async fn old_session(path: &Path) -> (uuid::Uuid, BTreeMap<&'static str, String>) {
    let file = SessionFile::create(path).await.unwrap();
    let s = file.session();
    s.set_tempo(&TempoMap::new(
        92.0,
        TimeSignature::new(4, 4),
        vec![
            TempoPoint::with_time_signature(
                Position::from_time(TimePosition::from_seconds(0.0)),
                92.0,
                TimeSignature::new(4, 4),
            ),
            TempoPoint::with_time_signature(
                Position::from_time(TimePosition::from_seconds(60.0)),
                120.0,
                TimeSignature::new(6, 8),
            ),
        ],
    ))
    .unwrap();
    s.set_bar_one(Some(Time::Seconds(2.6))).unwrap();
    s.put_marker(
        &MarkId("marker-1".into()),
        &Marker {
            at: Time::Seconds(2.6),
            name: "SONGSTART".into(),
            color: None,
            ruler: Some(0),
        },
    )
    .unwrap();
    s.put_region(
        &MarkId("region-1".into()),
        &Region {
            start: Time::Seconds(2.6),
            end: Time::Seconds(30.0),
            name: "Verse 1".into(),
            color: Some(0x33_66_99),
            ruler: Some(0),
        },
    )
    .unwrap();
    // Keyflow: its source text and a typed chord on its lane.
    let keyflow = ExtensionId::from("keyflow");
    s.ext(&keyflow)
        .unwrap()
        .set_text("chart", "Verse 1: C G Am F\n")
        .unwrap();
    s.put_lane(
        &LaneId("keyflow.chords".into()),
        &Lane {
            owner: keyflow.clone(),
            kind: "keyflow.chords".into(),
            name: "Chords".into(),
            order: 1.0,
            parent: None,
        },
    )
    .unwrap();
    s.put_item(
        &ItemId("chord-1".into()),
        &Item {
            owner: keyflow,
            lane: LaneId("keyflow.chords".into()),
            kind: "keyflow.chord".into(),
            at: Time::Seconds(2.6),
            end: Some(Time::Seconds(5.2)),
            payload: Value::Map(BTreeMap::from([("symbol".into(), Value::Text("C".into()))])),
        },
    )
    .unwrap();
    // Ignition: a cue list; Signal: a patch change on its lane.
    s.ext(&ExtensionId::from("ignition"))
        .unwrap()
        .set("cues", &Value::List(vec![Value::Text("Blackout".into())]))
        .unwrap();
    let signal = ExtensionId::from("signal");
    s.put_lane(
        &LaneId("signal.patches".into()),
        &Lane {
            owner: signal.clone(),
            kind: "signal.patch".into(),
            name: "Patches".into(),
            order: 2.0,
            parent: None,
        },
    )
    .unwrap();
    s.put_item(
        &ItemId("patch-1".into()),
        &Item {
            owner: signal,
            lane: LaneId("signal.patches".into()),
            kind: "signal.patch".into(),
            at: Time::Seconds(30.0),
            end: None,
            payload: Value::Text("Lead".into()),
        },
    )
    .unwrap();
    s.doc().commit();
    file.save().await.unwrap();
    let id = file.id();
    let want = BTreeMap::from([
        ("chart", "Verse 1: C G Am F\n".to_owned()),
        ("region", "Verse 1".to_owned()),
    ]);
    (id, want)
}

struct Fixture {
    _dir: tempfile::TempDir,
    song: PathBuf,
    trash: PathBuf,
    id: uuid::Uuid,
    project: serde_like::Project,
    session_value: loro::LoroValue,
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let song = dir.path().join("Songs").join("Drive To LA - JT");
    std::fs::create_dir_all(&song).unwrap();
    let project = old_project(&song.join(format!("{TITLE}.session")));
    let (id, _) = old_session(&song.join(format!("{TITLE}.fts-session"))).await;
    let session_value = SessionFile::open(&song.join(format!("{TITLE}.fts-session")))
        .await
        .unwrap()
        .session()
        .doc()
        .get_deep_value();
    Fixture {
        trash: dir.path().join("Trash"),
        _dir: dir,
        song,
        id,
        project,
        session_value,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_pair_becomes_one_file_with_everything_in_it() {
    let f = fixture().await;
    let before = names(&f.song);
    assert!(matches!(
        Layout::of(&f.song).unwrap(),
        Layout::Pair {
            project: Some(_),
            ..
        }
    ));

    // A dry run builds and verifies elsewhere, and changes nothing here.
    let dry = convert(
        &f.song,
        &Options {
            dry_run: true,
            trash_dir: None,
        },
    )
    .await
    .unwrap();
    let Outcome::Converted(dry) = dry else {
        panic!("a dry run converts: {dry:?}")
    };
    assert_eq!(dry.id, f.id.to_string());
    assert_eq!(names(&f.song), before);

    let options = Options {
        dry_run: false,
        trash_dir: Some(f.trash.clone()),
    };
    let Outcome::Converted(report) = convert(&f.song, &options).await.unwrap() else {
        panic!("converted")
    };
    assert_eq!(report.id, f.id.to_string());
    assert_eq!(report.project_branches, ["Acoustic", "main"]);
    let main = &report.sessions["main"];
    assert_eq!(
        (main.markers, main.regions, main.lanes, main.items),
        (1, 1, 2, 2)
    );
    assert!(main.bar_one);
    assert!(main.extensions.contains_key("keyflow"));
    assert!(main.extensions.contains_key("ignition"));

    // One file is left; the old pair is put away, never deleted.
    assert_eq!(names(&f.song), [format!("{TITLE}.session")]);
    let put_away = names(&f.trash);
    assert!(put_away.contains(&format!("{TITLE}.fts-session")));
    assert!(put_away.contains(&format!("{TITLE}.session")));
    assert!(put_away.contains(&format!("{TITLE}.session-wal")));

    // The one file is the song.
    let file = f.song.join(format!("{TITLE}.session"));
    assert_eq!(
        session_format::sniff_path(&file),
        session_format::FileFormat::Store
    );
    assert!(matches!(
        Layout::of(&f.song).unwrap(),
        Layout::OneFile { .. }
    ));
    let song = SongFile::open(&file).await.unwrap();
    assert_eq!(song.id().unwrap(), f.id);
    assert_eq!(
        song.session().session().doc().get_deep_value(),
        f.session_value
    );
    assert_eq!(
        song.session().id(),
        f.id,
        "the session syncs under the song's id"
    );
    let keyflow = song
        .session()
        .session()
        .read_ext(&ExtensionId::from("keyflow"));
    assert_eq!(keyflow.text("chart"), Some("Verse 1: C G Am F\n"));
    song.with_store(|s| {
        assert_eq!(s.load(MAIN).unwrap().get_deep_value(), f.project.main);
        assert_eq!(s.blob(&f.project.blob).unwrap(), b"a compressor's state");
        assert_eq!(s.history(MAIN, (0, 10)).unwrap().1, 1);
    });
    song.close().await.unwrap();

    // Again: nothing to do.
    assert!(matches!(
        convert(&f.song, &options).await.unwrap(),
        Outcome::AlreadyOneFile { .. }
    ));
    // Opening a store leaves its (empty, once closed) log and shared
    // memory beside it; the song is the `.session` alone.
    let left: Vec<String> = names(&f.song)
        .into_iter()
        .filter(|n| !n.ends_with("-tshm"))
        .collect();
    assert!(
        left.iter()
            .all(|n| n.starts_with(&format!("{TITLE}.session")))
    );
    let wal = f.song.join(format!("{TITLE}.session-wal"));
    assert!(
        !wal.exists() || std::fs::metadata(&wal).unwrap().len() == 0,
        "the log is folded in"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_one_file_opens_from_wherever_it_is_copied() {
    let f = fixture().await;
    let options = Options {
        dry_run: false,
        trash_dir: Some(f.trash.clone()),
    };
    convert(&f.song, &options).await.unwrap();
    // Edited and closed: the log folded in, the file alone the song.
    {
        let song = SongFile::open(&f.song.join(format!("{TITLE}.session")))
            .await
            .unwrap();
        song.session()
            .session()
            .set_bar_one(Some(Time::Seconds(4.0)))
            .unwrap();
        song.session().session().doc().commit();
        song.close().await.unwrap();
    }
    // Only the `.session` goes to the "external drive".
    let drive = tempfile::tempdir().unwrap();
    let carried = drive.path().join(format!("{TITLE}.session"));
    std::fs::copy(f.song.join(format!("{TITLE}.session")), &carried).unwrap();
    let read = session_format::read::ProjectFile::read(&std::fs::read(&carried).unwrap()).unwrap();
    assert_eq!(read.song_id(), Some(f.id.to_string().as_str()));
    let part = read.part(SESSION, MAIN).unwrap();
    assert_eq!(
        fts_session::doc::SessionDoc::from_doc(part).bar_one(),
        Some(Time::Seconds(4.0))
    );
    let song = SongFile::open(&carried).await.unwrap();
    assert_eq!(song.id().unwrap(), f.id);
    assert_eq!(song.session().session().bar_one(), Some(Time::Seconds(4.0)));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_is_read_by_its_headers_not_its_names() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(Layout::of(dir.path()).unwrap(), Layout::Empty);
    // A `.session` that is a text file, and a session document under
    // another name: the document is the session, the text is nothing.
    std::fs::write(dir.path().join("Notes.session"), b"not a store").unwrap();
    let doc = dir.path().join("Renamed.fts-session");
    SessionFile::create(&doc)
        .await
        .unwrap()
        .save()
        .await
        .unwrap();
    match Layout::of(dir.path()).unwrap() {
        Layout::Pair { project, session } => {
            assert_eq!(project, None);
            assert_eq!(session, doc);
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_conversion_stopped_before_the_trash_finishes_on_the_next_run() {
    let f = fixture().await;
    // Stopped after building and verifying: the Trash refused (here, a
    // file where its folder should be), so the new file waits beside the
    // old pair, which is untouched.
    let blocked = f.song.parent().unwrap().join("not a folder");
    std::fs::write(&blocked, b"").unwrap();
    let stopped = Options {
        dry_run: false,
        trash_dir: Some(blocked),
    };
    assert!(convert(&f.song, &stopped).await.is_err());
    let pending = names(&f.song);
    assert!(pending.contains(&format!("{TITLE}.session.new")));
    assert!(pending.contains(&format!("{TITLE}.fts-session")));
    let options = Options {
        dry_run: false,
        trash_dir: Some(f.trash.clone()),
    };
    let Outcome::Converted(report) = convert(&f.song, &options).await.unwrap() else {
        panic!()
    };
    assert_eq!(report.id, f.id.to_string());
    assert_eq!(names(&f.song), [format!("{TITLE}.session")]);
}
