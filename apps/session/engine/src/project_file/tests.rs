//! A song in a live set keeps one doc and one bridge: hosting shares the
//! song's own doc, and joining — a copy of the same song, or a song another
//! peer seeded the set with — goes through the song's project file, never a
//! second bridge on its engine (#316).

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use web_time::Instant;

use daw::standalone::Standalone;
use session::sync::relay::{Side, exchange, same_lineage, seed};
use session::sync::{Bridge, SessionDoc};
use session_format::store::MAIN;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .expect("a runtime")
}

/// Song `guid` seeded into `daw` with one track, `Verb`, holding one FX.
fn seed_song(daw: &Standalone, guid: &str) {
    daw.seed_project(daw_proto::ProjectInfo {
        guid: guid.into(),
        name: guid.into(),
        path: String::new(),
    });
    runtime().block_on(async {
        let facade = daw::standalone::bootstrap::build_in_process_daw(daw.clone())
            .await
            .expect("a facade");
        let project = facade.daw.project(guid.to_owned()).await.expect("the song");
        let track = project
            .tracks()
            .add_with_guid("{VERB}", "Verb", None)
            .await
            .expect("a track");
        track.fx_chain().add("ReaVerb").await.expect("an FX");
    });
}

fn fx_in(daw: &Standalone, guid: &str) -> usize {
    daw.read_project(guid, |p| p.fx_chains.values().map(Vec::len).sum())
        .expect("the project")
}

fn verb_volume(daw: &Standalone, guid: &str) -> Option<f64> {
    daw.read_project(guid, |p| {
        p.tracks.iter().find(|t| t.name == "Verb").map(|t| t.volume)
    })
    .flatten()
}

/// Whether a second bridge on `guid` is refused: it has its one.
fn has_its_one_bridge(daw: &Standalone, guid: &str) -> bool {
    runtime().block_on(async {
        let facade = daw::standalone::bootstrap::build_in_process_daw(daw.clone())
            .await
            .expect("a facade");
        let project = facade.daw.project(guid.to_owned()).await.expect("the song");
        let second = Bridge::host(
            project,
            session::sync::engine::MediaRoot(std::path::PathBuf::from("/")),
            SessionDoc::new(),
            String::new(),
            Arc::new(session::sync::engine::MemoryBlobs::default()),
            Some(Arc::new(daw.clone())),
        )
        .await;
        second.is_err_and(|e| {
            e.downcast_ref::<session::sync::bridge::AlreadyBridged>()
                .is_some()
        })
    })
}

/// Wait (up to 10 s) until `what` holds.
fn until(what: impl Fn() -> bool, why: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !what() {
        assert!(Instant::now() < deadline, "{why}: never came");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn pass(a: &Side, b: &Side) {
    runtime()
        .block_on(exchange(a, b))
        .expect("the docs passed what each lacked");
}

fn saved(daw: &Standalone, guid: &str, file: &Path) {
    super::save(daw, guid, file, MAIN, "Save").expect("saved");
}

/// A joiner's own mix, told apart from the host's.
const OWN_MIX: f64 = 0.33;

#[test]
fn a_hosted_set_is_joined_through_the_project_file_with_one_bridge_a_song() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let daw = Standalone::new();

    // The host's song, open from its file: its one bridge.
    seed_song(&daw, "host-song");
    let host_file = dir.path().join("Host.session");
    saved(&daw, "host-song", &host_file);
    assert!(super::is_open("host-song"));
    let fx = fx_in(&daw, "host-song");
    assert_eq!(fx, 1);

    // Hosted: the live doc seeded from the song's own doc.
    let host = super::live_side(&daw, "host-song", &host_file).expect("its doc");
    let live = SessionDoc::new();
    live.loro()
        .import_batch(&seed(&host.doc).expect("a seed"))
        .expect("seeded");
    let live_side = Side::imported(live.loro().clone());
    assert!(
        has_its_one_bridge(&daw, "host-song"),
        "hosting made no second bridge"
    );

    // A peer with a copy of the same file joins: the same lineage, relayed.
    let copy_file = dir.path().join("Copy.session");
    std::fs::copy(&host_file, &copy_file).expect("a copy");
    let copy = super::load(&daw, "Copy", &copy_file, MAIN).expect("opened from its file");
    let copy_side = super::live_side(&daw, &copy, &copy_file).expect("its doc");
    assert!(same_lineage(&copy_side.doc, live.loro()));
    pass(&copy_side, &live_side);

    // A peer with its own copy of the song (another lineage), its own mix
    // on it and an alternative made from it, joins: moved onto the set's
    // doc, through its file — its own version kept in that file.
    seed_song(&daw, "own-song");
    daw.write_project("own-song", |p| {
        if let Some(t) = p.tracks.iter_mut().find(|t| t.name == "Verb") {
            t.volume = OWN_MIX;
        }
    });
    let own_file = dir.path().join("Own.session");
    saved(&daw, "own-song", &own_file);
    super::fork(&daw, "own-song", &own_file, "Organized").expect("an alternative");
    let own_side = super::live_side(&daw, "own-song", &own_file).expect("its doc");
    assert!(!same_lineage(&own_side.doc, live.loro()));
    let archive = super::archive_name("Sunday / Night", "2026-10-29");
    assert_eq!(archive, "Before joining Sunday - Night (2026-10-29)");
    let adopted =
        super::adopt("own-song", live.loro(), &archive).expect("moved onto the set's doc");
    assert_eq!(adopted.kept_as, archive);
    let own_side = adopted.side;
    assert!(same_lineage(&own_side.doc, live.loro()));
    pass(&own_side, &live_side);
    assert_eq!(
        verb_volume(&daw, "own-song"),
        verb_volume(&daw, "host-song"),
        "the engine is the set's song now"
    );
    // Nothing of its own version is lost: kept as a branch of its file,
    // listed with its alternatives, and opening as it was — as is the
    // alternative made before, though the song it was forked from started
    // over.
    let names: Vec<String> = super::branches(&own_file)
        .expect("its branches")
        .into_iter()
        .map(|b| b.name)
        .collect();
    assert!(names.contains(&archive), "kept as {archive}: {names:?}");
    for (branch, why) in [
        (archive.as_str(), "the kept version"),
        ("Organized", "the alternative"),
    ] {
        let doc = super::doc_of(&own_file.join(branch)).unwrap_or_else(|| panic!("{why} opens"));
        let volume = SessionDoc::from_loro(doc)
            .read()
            .tracks
            .iter()
            .find(|t| t.name == "Verb")
            .map(|t| t.volume);
        assert_eq!(volume, Some(OWN_MIX), "{why} holds its own mix");
    }
    let main = super::doc_of(&own_file).expect("the song opens");
    assert!(
        same_lineage(&main, live.loro()),
        "the song's file is of the set's lineage now"
    );

    for guid in ["host-song", copy.as_str(), "own-song"] {
        assert!(has_its_one_bridge(&daw, guid), "{guid}: one bridge");
        assert_eq!(fx_in(&daw, guid), fx, "{guid}: built once");
    }

    // Edits on the host, round after round, reach both peers' engines
    // through their files' bridges; nothing is made twice.
    for round in 1..=6_u32 {
        let volume = f64::from(round) / 10.0;
        daw.write_project("host-song", |p| {
            if let Some(t) = p.tracks.iter_mut().find(|t| t.name == "Verb") {
                t.volume = volume;
            }
        });
        saved(&daw, "host-song", &host_file);
        pass(&host, &live_side);
        pass(&live_side, &copy_side);
        pass(&live_side, &own_side);
        for guid in [copy.as_str(), "own-song"] {
            until(
                || verb_volume(&daw, guid) == Some(volume),
                &format!("round {round}: {guid} follows the host"),
            );
        }
        for guid in ["host-song", copy.as_str(), "own-song"] {
            assert_eq!(fx_in(&daw, guid), fx, "round {round}: {guid}'s FX");
        }
    }
    assert_eq!(
        live.read().tracks.iter().map(|t| t.fx.len()).sum::<usize>(),
        fx
    );
}

/// A copy saved elsewhere is written once: the song keeps its one bridge,
/// and another can still not be made on it.
#[test]
fn a_copy_saved_elsewhere_makes_no_second_bridge() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let daw = Standalone::new();
    seed_song(&daw, "copied-song");
    let file = dir.path().join("Song.session");
    saved(&daw, "copied-song", &file);
    let elsewhere = dir.path().join("Elsewhere.session");
    saved(&daw, "copied-song", &elsewhere);
    let doc = super::doc_of(&elsewhere).expect("the copy holds the song");
    assert_eq!(
        SessionDoc::from_loro(doc)
            .read()
            .tracks
            .iter()
            .map(|t| t.fx.len())
            .sum::<usize>(),
        1
    );
    assert!(has_its_one_bridge(&daw, "copied-song"));
}

#[test]
fn a_date_is_the_calendar_day() {
    assert_eq!(super::date_of(0), "1970-01-01");
    assert_eq!(super::date_of(951_782_400), "2000-02-29");
    assert_eq!(super::date_of(1_793_232_000), "2026-10-29");
}

/// An `.RPP` is a session file from the moment it opens: `Song.session`
/// beside it, the song open from it (its one bridge), the `.RPP` as it
/// was — and the next open is the `.session`.
#[test]
fn an_rpp_opens_as_its_session_file() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let rpp = dir.path().join("Washed.RPP");
    let text = "<REAPER_PROJECT 0.1 \"7.0/test\" 1700000000\n  SAMPLERATE 48000 0 0\n  TEMPO 120 4 4 0\n  <TRACK {0B5E0000-0000-0000-0000-000000000001}\n    NAME Guitar\n  >\n>\n";
    std::fs::write(&rpp, text).expect("an .RPP");
    let daw = Standalone::new();
    let (opened, _) = crate::open_core::open_song_in(
        crate::folder::disk(),
        &daw,
        &rpp,
        &crate::prepare::Prepare::default(),
        crate::open_core::Media::Deferred,
    )
    .expect("opened");
    let session = dir.path().join("Washed.session");
    assert!(
        super::is_project_file(&session),
        "its session file is there"
    );
    assert!(
        super::is_open(&opened.project_guid),
        "the song is open from it"
    );
    assert!(has_its_one_bridge(&daw, &opened.project_guid));
    assert_eq!(
        std::fs::read_to_string(&rpp).expect("the .RPP"),
        text,
        "the .RPP is left as it was"
    );
    let names: Vec<String> = SessionDoc::from_loro(super::doc_of(&session).expect("its doc"))
        .read()
        .tracks
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, ["Guitar"]);
    let plan = crate::open_core::song_plan_in(
        crate::folder::disk(),
        &rpp,
        &crate::prepare::Prepare::default(),
    );
    assert_eq!(
        plan.open, session,
        "it opens as its session file from now on"
    );
}
