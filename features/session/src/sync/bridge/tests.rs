//! A song built on a stage and landed ([`Bridge::moved_to`]) is the song
//! built in place: the same engine state, field for field, and a bridge
//! that keeps it as the one built in place does.
//!
//! The real-song cases read `FTS_OPEN_SONG` (a `Song.session`; copied to a
//! temp dir first, since opening a store writes beside it) and are skipped
//! without it:
//!
//! ```text
//! FTS_OPEN_SONG=/…/fire/Fire.session cargo nextest run -p session staged
//! ```

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use daw_proto::ProjectInfo;
use daw_standalone::sync::{ProjectState, Standalone};

use crate::sync::engine::{self, Blobs, Direct, EngineIds, MediaRoot, MemoryBlobs};
use crate::sync::model::{
    EnvelopePointState, EnvelopeState, FxState, ItemState, MarkerState, NoteState, RegionState,
    SendState, SessionModel, TakeState, TempoPoint, TrackState,
};
use crate::sync::{Bridge, ORIGIN_LOCAL, SessionDoc};

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .expect("a runtime")
}

fn info(guid: &str) -> ProjectInfo {
    ProjectInfo {
        guid: guid.into(),
        name: guid.into(),
        path: String::new(),
    }
}

fn direct(daw: &Standalone) -> Option<Arc<dyn Direct>> {
    Some(Arc::new(daw.clone()))
}

/// What a song needs to be opened: its doc, where its media is, its blobs.
struct Song {
    doc: SessionDoc,
    media: MediaRoot,
    blobs: Arc<dyn Blobs>,
}

/// `song` opened into `daw` as project `guid`, rebuilt in place.
async fn in_place(daw: &Standalone, guid: &str, song: &Song) -> Bridge {
    daw.seed_project(info(guid));
    let facade = daw_standalone::bootstrap::build_in_process_daw(daw.clone())
        .await
        .expect("a facade");
    let project = facade.daw.project(guid.to_owned()).await.expect("the song");
    Bridge::join(
        project,
        song.media.clone(),
        song.doc.fork(),
        song.blobs.clone(),
        direct(daw),
    )
    .await
    .expect("joined in place")
}

/// `song` opened into `daw` as project `guid`, built on a stage and
/// landed. Its facade on the stage is dropped after.
async fn staged(daw: &Standalone, guid: &str, song: &Song) -> Bridge {
    let stage = daw.stage();
    stage.engine().seed_project(info(guid));
    let built = {
        let facade = daw_standalone::bootstrap::build_in_process_daw(stage.engine().clone())
            .await
            .expect("a facade on the stage");
        let project = facade.daw.project(guid.to_owned()).await.expect("the song");
        Bridge::join(
            project,
            song.media.clone(),
            song.doc.fork(),
            song.blobs.clone(),
            direct(stage.engine()),
        )
        .await
        .expect("joined on the stage")
    };
    stage.land(guid).expect("landed");
    let facade = daw_standalone::bootstrap::build_in_process_daw(daw.clone())
        .await
        .expect("a facade");
    let project = facade.daw.project(guid.to_owned()).await.expect("the song");
    built.moved_to(project, direct(daw))
}

/// Everything project `guid` holds, by part, as text — with each take's
/// and FX's guid (made fresh each time one is made) named by where it is
/// instead, and every map in key order.
/// An item made under a guid of the engine's own (a MIDI item) is named
/// by its doc id, through `ids`.
fn engine_view(daw: &Standalone, guid: &str, ids: &EngineIds) -> BTreeMap<&'static str, String> {
    daw.read_project(guid, |p| view_of(p, ids))
        .expect("the project")
}

fn view_of(p: &ProjectState, ids: &EngineIds) -> BTreeMap<&'static str, String> {
    let mut takes: Vec<(String, String)> = Vec::new();
    for (item, list) in p.takes.iter() {
        for (place, take) in list.takes.iter().enumerate() {
            takes.push((take.guid.clone(), format!("<{item} take {place}>")));
        }
    }
    for (chain, entries) in &p.fx_chains {
        for (place, entry) in entries.iter().enumerate() {
            takes.push((entry.fx.guid.clone(), format!("<{chain:?} fx {place}>")));
        }
    }
    for item in p.items.keys() {
        let doc = ids.items.doc(item);
        if doc != *item {
            takes.push((item.clone(), format!("<item {doc}>")));
        }
    }
    let named = |text: String| {
        takes
            .iter()
            .fold(text, |text, (guid, name)| text.replace(guid.as_str(), name))
    };
    let sorted = |entries: Vec<(String, String)>| {
        let map: BTreeMap<String, String> = entries
            .into_iter()
            .map(|(k, v)| (named(k), named(v)))
            .collect();
        format!("{map:#?}")
    };
    fn each<K: Debug, V: Debug>(map: impl IntoIterator<Item = (K, V)>) -> Vec<(String, String)> {
        map.into_iter()
            .map(|(k, v)| (format!("{k:?}"), format!("{v:?}")))
            .collect()
    }
    let fx = p
        .fx_chains
        .iter()
        .map(|(chain, entries)| {
            let entries: Vec<String> = entries
                .iter()
                .map(|e| {
                    let params: BTreeMap<_, _> = e.params.iter().collect();
                    format!("{:?} {:?} {params:?} {:?}", e.fx, e.state_chunk, e.ranges)
                })
                .collect();
            (format!("{chain:?}"), format!("{entries:#?}"))
        })
        .collect();
    let mut view = BTreeMap::new();
    view.insert("info", format!("{:?}", p.info));
    view.insert("transport", format!("{:?}", p.transport));
    view.insert("tracks", named(format!("{:#?}", p.tracks)));
    view.insert("track_ext", sorted(each(&p.track_ext)));
    view.insert(
        "items",
        sorted(each(p.items.iter().map(|(k, v)| (k, &v.item)))),
    );
    view.insert("items_by_track", sorted(each(p.items_by_track.iter())));
    view.insert("takes", sorted(each(p.takes.iter())));
    view.insert("midi_notes", sorted(each(p.midi_notes.iter())));
    view.insert("midi_ccs", sorted(each(p.midi_ccs.iter())));
    view.insert("stretch_markers", sorted(each(p.stretch_markers.iter())));
    view.insert("stretch_modes", sorted(each(p.stretch_modes.iter())));
    view.insert("take_markers", sorted(each(p.take_markers.iter())));
    view.insert("envelopes", sorted(each(p.envelopes.iter())));
    view.insert("markers", format!("{:#?}", p.markers));
    view.insert("regions", format!("{:#?}", p.regions));
    view.insert("ruler_lanes", format!("{:#?}", p.ruler_lanes));
    view.insert("tempo", format!("{:#?}", p.tempo_points));
    view.insert("fx", sorted(fx));
    view.insert("sends", sorted(each(&p.sends)));
    view.insert("receives", sorted(each(&p.receives)));
    view.insert("hw_outputs", sorted(each(&p.hw_outputs)));
    view.insert("track_ext_state", sorted(each(&p.track_ext_state)));
    view.insert("project_ext_state", sorted(each(&p.project_ext_state)));
    view.insert(
        "master",
        format!("{} {} {}", p.master_volume, p.master_pan, p.master_muted),
    );
    view.insert(
        "counters",
        format!(
            "{} {} {} {} {}",
            p.next_region_id,
            p.next_marker_id,
            p.next_item_counter,
            p.next_fx_counter,
            p.next_take_counter
        ),
    );
    view
}

/// Opened both ways, the two songs are the same, part for part, and read
/// back the same; and the landed song's bridge records an edit made to it
/// where it now is.
async fn opens_alike(song: &Song) {
    let daw = Standalone::new();
    let mut here = in_place(&daw, "in-place", song).await;
    let mut there = staged(&daw, "staged", song).await;

    let built = engine_view(&daw, "in-place", &here.ids);
    let landed = engine_view(&daw, "staged", &there.ids);
    assert!(!built["tracks"].is_empty());
    for (part, text) in &built {
        let built = text.replace("in-place", "<song>");
        let landed = landed[part].replace("staged", "<song>");
        let differ = built
            .lines()
            .zip(landed.lines())
            .find(|(a, b)| a != b)
            .map(|(a, b)| format!("built   {a}\nlanded  {b}"));
        assert_eq!(differ, None, "{part} differ");
        assert_eq!(built.len(), landed.len(), "{part} differ in length");
    }
    assert_eq!(here.shown, there.shown, "both read back alike");

    // The landed bridge keeps the landed song: an edit there is a step.
    let first = song.doc.read().tracks.first().map(|t| t.guid.clone());
    if let Some(track) = first {
        for (bridge, guid) in [(&mut here, "in-place"), (&mut there, "staged")] {
            daw.write_project(guid, |p| {
                if let Some(t) = p.tracks.iter_mut().find(|t| t.guid == track) {
                    t.volume = 0.25;
                }
            });
            assert!(bridge.local_step("Turn it down").await.expect("a step"));
            let doc = bridge.doc().read();
            let volume = doc
                .tracks
                .iter()
                .find(|t| t.guid == track)
                .map(|t| t.volume);
            assert_eq!(volume, Some(0.25), "{guid}'s step reached its doc");
        }
    }
}

/// A small song with a bit of everything the doc holds: a folder and its
/// tracks, FX, a send, an envelope, audio and MIDI items, markers, a
/// region and a tempo change.
fn synthetic() -> SessionModel {
    let track = |guid: &str, parent: Option<&str>| TrackState {
        guid: guid.into(),
        parent: parent.map(Into::into),
        name: guid.to_uppercase(),
        volume: 0.8,
        visible_in_tcp: true,
        visible_in_mixer: true,
        parent_send: true,
        channels: 2,
        automation_mode: "trim_read".into(),
        ..TrackState::default()
    };
    let mut drums = track("drums", None);
    drums.fx.push(FxState {
        guid: "{EQ}".into(),
        plugin: "ReaEQ".into(),
        name: "ReaEQ".into(),
        enabled: true,
        ..FxState::default()
    });
    let mut kick = track("kick", Some("drums"));
    kick.sends.push(SendState {
        dest: "verb".into(),
        mode: "post_fader".into(),
        volume: 0.5,
        ..SendState::default()
    });
    kick.envelopes.insert(
        "volume".into(),
        EnvelopeState {
            mode: "read".into(),
            visible: true,
            points: vec![
                EnvelopePointState {
                    at: 0.0,
                    value: 1.0,
                    ..EnvelopePointState::default()
                },
                EnvelopePointState {
                    at: 2.0,
                    value: 0.5,
                    ..EnvelopePointState::default()
                },
            ],
            ..EnvelopeState::default()
        },
    );
    let tracks = vec![
        drums,
        kick,
        track("snare", Some("drums")),
        track("verb", None),
    ];
    let audio = |track: &str, at: f64, source: &str| ItemState {
        track: track.into(),
        position: at,
        length: 1.5,
        volume: 1.0,
        fade_in: 0.01,
        fade_in_shape: "linear".into(),
        fade_out_shape: "linear".into(),
        take: TakeState {
            name: source.into(),
            source: Some(format!("Audio/{source}.wav")),
            playrate: 1.0,
            ..TakeState::default()
        },
        ..ItemState::default()
    };
    let mut items = BTreeMap::new();
    for (n, at) in [0.0, 2.0, 4.0].into_iter().enumerate() {
        items.insert(format!("kick-{n}"), audio("kick", at, "Kick"));
        items.insert(format!("snare-{n}"), audio("snare", at + 1.0, "Snare"));
    }
    items.insert(
        "groove".into(),
        ItemState {
            track: "verb".into(),
            position: 0.0,
            length: 4.0,
            volume: 1.0,
            fade_in_shape: "linear".into(),
            fade_out_shape: "linear".into(),
            take: TakeState {
                midi: true,
                playrate: 1.0,
                notes: vec![NoteState {
                    start: 0.0,
                    length: 0.5,
                    pitch: 36,
                    velocity: 100,
                    channel: 0,
                    muted: false,
                }],
                ..TakeState::default()
            },
            ..ItemState::default()
        },
    );
    SessionModel {
        tracks,
        items,
        markers: BTreeMap::from([(
            "{M1}".to_owned(),
            MarkerState {
                at: 1.0,
                name: "Verse".into(),
                ..MarkerState::default()
            },
        )]),
        regions: BTreeMap::from([(
            "{R1}".to_owned(),
            RegionState {
                start: 0.0,
                end: 4.0,
                name: "Intro".into(),
                ..RegionState::default()
            },
        )]),
        tempo: vec![
            TempoPoint {
                at: 0.0,
                bpm: 120.0,
                beats_per_bar: 4,
                beat_unit: 4,
            },
            TempoPoint {
                at: 4.0,
                bpm: 96.0,
                beats_per_bar: 3,
                beat_unit: 4,
            },
        ],
        ..SessionModel::default()
    }
}

#[test]
fn a_small_song_staged_is_the_song_built_in_place() {
    let doc = SessionDoc::new();
    doc.write(&synthetic(), ORIGIN_LOCAL).expect("written");
    let song = Song {
        doc,
        media: MediaRoot(PathBuf::from("/song")),
        blobs: Arc::new(MemoryBlobs::default()),
    };
    runtime().block_on(opens_alike(&song));
}

/// How many things project `guid`'s engine has made since it opened —
/// items, FX and takes, each counted as the engine mints its guid — and
/// every item and FX guid it holds now: a song built again shows both
/// moving.
fn made(daw: &Standalone, guid: &str) -> (u64, u64, u64, Vec<String>) {
    daw.read_project(guid, |p| {
        let mut guids: Vec<String> = p.items.keys().cloned().collect();
        for entries in p.fx_chains.values() {
            guids.extend(entries.iter().map(|e| e.fx.guid.clone()));
        }
        guids.sort();
        (
            p.next_item_counter,
            p.next_fx_counter,
            p.next_take_counter,
            guids,
        )
    })
    .expect("the project")
}

/// `doc` joined on the song already built into `daw` as `guid`, the way
/// a song joins a live set (a fresh bridge, as collab makes one).
async fn join_built(daw: &Standalone, guid: &str, song: &Song, doc: SessionDoc) -> Bridge {
    let facade = daw_standalone::bootstrap::build_in_process_daw(daw.clone())
        .await
        .expect("a facade");
    let project = facade.daw.project(guid.to_owned()).await.expect("the song");
    Bridge::join(
        project,
        song.media.clone(),
        doc,
        song.blobs.clone(),
        direct(daw),
    )
    .await
    .expect("joined the live doc")
}

/// What a peer that built `song` itself seeds a live set's doc with: its
/// engine read afresh (so its FX and MIDI items under that engine's own
/// guids, not the file's ids), as `collab` hosts a song.
async fn seeded_elsewhere(song: &Song) -> SessionDoc {
    let daw = Standalone::new();
    let _built = in_place(&daw, "host", song).await;
    let facade = daw_standalone::bootstrap::build_in_process_daw(daw.clone())
        .await
        .expect("a facade");
    let project = facade
        .daw
        .project("host".to_owned())
        .await
        .expect("the song");
    let seed = SessionDoc::new();
    let host = Bridge::host(
        project,
        song.media.clone(),
        seed,
        song.doc.read().chart,
        song.blobs.clone(),
        direct(&daw),
    )
    .await
    .expect("seeded");
    host.doc().fork()
}

/// The engine parts a build makes, with its counters and the order its
/// items were made in on each track left out (neither is the doc's: a
/// song built twice counts twice, and is the same song).
fn built_parts(daw: &Standalone, guid: &str, ids: &EngineIds) -> BTreeMap<&'static str, String> {
    let mut view = engine_view(daw, guid, ids);
    view.remove("counters");
    // Which track each item is on is in `items`; the lists only add the
    // order they were made in.
    view.remove("items_by_track");
    if let Some(items) = view.get_mut("items") {
        *items = without_item_order(items);
    }
    view
}

/// `text` with every item's `index: <n>, ` taken out.
fn without_item_order(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("index: ") {
        out.push_str(&rest[..at]);
        let after = &rest[at + "index: ".len()..];
        let digits = after.len() - after.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        rest = after[digits..]
            .strip_prefix(", ")
            .unwrap_or(&after[digits..]);
    }
    out.push_str(rest);
    out
}

/// A song built from its file and then joined to a live set whose doc
/// holds the same song is built once: the join makes nothing in the
/// engine (no item, FX or take minted, the same guids held), the engine
/// is what a build straight from the live doc makes, and the bridge
/// shows what the doc holds — whether the doc is the file's own history
/// or one another peer seeded from its own build (its FX and MIDI items
/// under other guids).
async fn joins_without_building(song: &Song) {
    let seeded = seeded_elsewhere(song).await;
    for (case, live) in [
        ("the file's doc", song.doc.fork()),
        ("a peer's seed", seeded),
    ] {
        let daw = Standalone::new();
        let built = in_place(&daw, "joined-song", song).await;
        let before = made(&daw, "joined-song");
        let joined = join_built(&daw, "joined-song", song, live.fork()).await;
        assert_eq!(
            made(&daw, "joined-song"),
            before,
            "{case}: the join built again"
        );

        let fresh = Song {
            doc: live.fork(),
            media: song.media.clone(),
            blobs: song.blobs.clone(),
        };
        let straight = in_place(&daw, "straight-song", &fresh).await;
        let have = built_parts(&daw, "joined-song", &joined.ids);
        let want = built_parts(&daw, "straight-song", &straight.ids);
        for (part, text) in &want {
            let have = have[part].replace("joined-song", "<song>");
            let want = text.replace("straight-song", "<song>");
            let differ = have
                .lines()
                .zip(want.lines())
                .find(|(a, b)| a != b)
                .map(|(a, b)| format!("joined    {a}\nstraight  {b}"));
            assert_eq!(differ, None, "{case}: {part} differ");
            assert_eq!(have.len(), want.len(), "{case}: {part} differ in length");
        }
        let mut doc = live.read();
        doc.chart.clone_from(&joined.shown().chart);
        assert_eq!(joined.shown(), &doc, "{case}: the bridge shows the doc");
        drop(built);
    }
}

/// The bridge that built a song, re-pointed at a live doc of the same
/// song ([`Bridge::rejoin`]), makes nothing in the engine either, and
/// shows what the doc holds.
async fn rejoins_without_building(song: &Song) {
    let seeded = seeded_elsewhere(song).await;
    for (case, live) in [
        ("the file's doc", song.doc.fork()),
        ("a peer's seed", seeded),
    ] {
        let daw = Standalone::new();
        let built = in_place(&daw, "rejoined-song", song).await;
        let before = made(&daw, "rejoined-song");
        let joined = built.rejoin(live.fork()).await.expect("rejoined");
        assert_eq!(
            made(&daw, "rejoined-song"),
            before,
            "{case}: the rejoin built again"
        );
        let mut doc = live.read();
        doc.chart.clone_from(&joined.shown().chart);
        assert_eq!(joined.shown(), &doc, "{case}: the bridge shows the doc");
    }
}

#[test]
fn a_small_song_rejoined_to_its_live_doc_is_built_once() {
    let doc = SessionDoc::new();
    doc.write(&synthetic(), ORIGIN_LOCAL).expect("written");
    let written = Song {
        doc,
        media: MediaRoot(PathBuf::from("/song")),
        blobs: Arc::new(MemoryBlobs::default()),
    };
    runtime().block_on(async {
        let song = Song {
            doc: seeded_elsewhere(&written).await,
            ..written
        };
        rejoins_without_building(&song).await;
    });
}

#[test]
fn a_small_song_joined_to_its_live_doc_is_built_once() {
    let doc = SessionDoc::new();
    doc.write(&synthetic(), ORIGIN_LOCAL).expect("written");
    let written = Song {
        doc,
        media: MediaRoot(PathBuf::from("/song")),
        blobs: Arc::new(MemoryBlobs::default()),
    };
    runtime().block_on(async {
        // Its file as a host saves one: what its engine read back.
        let song = Song {
            doc: seeded_elsewhere(&written).await,
            ..written
        };
        joins_without_building(&song).await;
    });
}

#[test]
fn a_real_song_joined_to_its_live_doc_is_built_once() {
    let scratch = tempfile::tempdir().expect("a temp dir");
    let Some(song) = real_song(scratch.path()) else {
        return;
    };
    runtime().block_on(joins_without_building(&song));
}

/// A real song (`FTS_OPEN_SONG`), opened from a copy of its file.
fn real_song(scratch: &Path) -> Option<Song> {
    let original = PathBuf::from(std::env::var_os("FTS_OPEN_SONG")?);
    let copy = scratch.join(original.file_name()?);
    std::fs::copy(&original, &copy).expect("a copy");
    let wal = PathBuf::from(format!("{}-wal", original.display()));
    if wal.exists() {
        std::fs::copy(&wal, format!("{}-wal", copy.display())).expect("its log");
    }
    let store = session_format::store::Store::open(&copy).expect("the store");
    let doc = SessionDoc::from_loro(store.load(session_format::store::MAIN).expect("the song"));
    Some(Song {
        doc,
        // Where the original's media is: read only.
        media: MediaRoot(original.parent()?.to_path_buf()),
        blobs: Arc::new(StoreBlobs(Mutex::new(store))),
    })
}

struct StoreBlobs(Mutex<session_format::store::Store>);

impl Blobs for StoreBlobs {
    fn put(&self, bytes: &[u8]) -> String {
        engine::blob_name(bytes)
    }
    fn get(&self, name: &str) -> Option<Vec<u8>> {
        self.0.lock().ok()?.blob(name).ok()
    }
}

#[test]
fn a_real_song_staged_is_the_song_built_in_place() {
    let scratch = tempfile::tempdir().expect("a temp dir");
    let Some(song) = real_song(scratch.path()) else {
        return;
    };
    runtime().block_on(opens_alike(&song));
}

/// While a real song is built on a stage and landed, a reader of another
/// song in the engine never waits more than 20 ms for it (the budget is
/// for an optimised build: `--profile release-fast`).
#[test]
fn a_real_song_staged_never_stalls_another_songs_reader() {
    let scratch = tempfile::tempdir().expect("a temp dir");
    let Some(song) = real_song(scratch.path()) else {
        return;
    };
    let daw = Standalone::new();
    daw.seed_project(info("playing"));
    let done = std::sync::atomic::AtomicBool::new(false);
    let worst = std::thread::scope(|scope| {
        scope.spawn(|| {
            runtime().block_on(async {
                let _bridge = staged(&daw, "opening", &song).await;
            });
            done.store(true, std::sync::atomic::Ordering::Release);
        });
        let mut worst = Duration::ZERO;
        while !done.load(std::sync::atomic::Ordering::Acquire) {
            let asked = Instant::now();
            let found = daw.read_project("playing", |p| p.tracks.len());
            worst = worst.max(asked.elapsed());
            assert_eq!(found, Some(0));
        }
        worst
    });
    assert!(daw.read_project("opening", |p| p.tracks.len()) > Some(0));
    assert!(
        worst < Duration::from_millis(20),
        "a read of another song waited {worst:?}"
    );
}

/// A step after the first reads only what the engine's journals say
/// changed — an item moved, one removed, a recording's markers written —
/// and comes to the very model a whole read gives.
#[test]
fn a_step_read_from_the_journal_is_the_engine_read_whole() {
    let doc = SessionDoc::new();
    doc.write(&synthetic(), ORIGIN_LOCAL).expect("written");
    let song = Song {
        doc,
        media: MediaRoot(PathBuf::from("/song")),
        blobs: Arc::new(MemoryBlobs::default()),
    };
    runtime().block_on(async {
        let daw = Standalone::new();
        let mut bridge = in_place(&daw, "song", &song).await;
        // The first step reads whole and leaves the bridge its cursor.
        daw.write_project("song", |p| {
            if let Some(t) = p.tracks.first_mut() {
                t.volume = 0.5;
            }
        });
        assert!(bridge.local_step("Turn it down").await.expect("a step"));
        assert!(bridge.cursor.is_some(), "read whole, the journal from here");

        let (moved, gone, source) = daw
            .read_project("song", |p| {
                let mut kicks: Vec<&String> =
                    p.items.keys().filter(|k| k.contains("kick")).collect();
                kicks.sort();
                let source = p
                    .takes
                    .get(kicks[0])
                    .and_then(|l| l.takes.first())
                    .and_then(|t| t.source_file_path.clone())
                    .expect("a recording");
                (kicks[0].clone(), kicks[1].clone(), source)
            })
            .expect("the song");
        daw.write_project("song", |p| {
            if let Some(entry) = p.items.get_mut(&moved) {
                entry.item.position = daw_proto::primitives::PositionInSeconds::from_seconds(7.25);
            }
            p.items.remove(&gone);
            p.take_markers
                .entry(source)
                .or_default()
                .push(daw_proto::TakeMarker {
                    id: "hit-1".into(),
                    name: "kick".into(),
                    source_position_seconds: 0.5,
                    ..daw_proto::TakeMarker::default()
                });
        });
        assert!(bridge.local_step("Edit the kick").await.expect("a step"));

        let whole = engine::read(
            &bridge.project,
            &bridge.media,
            bridge.blobs.as_ref(),
            &bridge.ids,
            bridge.direct.as_deref(),
        )
        .await
        .expect("read whole");
        assert_eq!(bridge.shown.items, whole.items, "items");
        assert_eq!(
            bridge.shown.take_markers, whole.take_markers,
            "take markers"
        );
        assert!(!whole.take_markers.is_empty(), "the marker was read");
        assert!(
            !bridge.shown.items.contains_key(&gone),
            "the removed item is gone"
        );
    });
}
