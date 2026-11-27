//! A song in a live set keeps one doc and one bridge: hosting shares the
//! song's own doc, and joining — a copy of the same song, or a song another
//! peer seeded the set with — goes through the song's project file, never a
//! second bridge on its engine (#316).

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use web_time::Instant;

use daw::standalone::Standalone;
use engine_sync::relay::{Side, exchange, same_lineage, seed};
use engine_sync::{Bridge, SessionDoc};
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
            engine_sync::engine::MediaRoot(std::path::PathBuf::from("/")),
            SessionDoc::new(),
            String::new(),
            Arc::new(engine_sync::engine::MemoryBlobs::default()),
            Some(Arc::new(daw.clone())),
        )
        .await;
        second.is_err_and(|e| {
            e.downcast_ref::<engine_sync::bridge::AlreadyBridged>()
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

/// The Verb track's volume set to `volume` in `guid`'s engine.
fn set_verb(daw: &Standalone, guid: &str, volume: f64) {
    daw.write_project(guid, |p| {
        if let Some(t) = p.tracks.iter_mut().find(|t| t.name == "Verb") {
            t.volume = volume;
        }
    });
}

#[test]
fn a_mark_waits_on_no_write_and_lands_between_the_steps_around_it() {
    // A step read and handed to the writer, a mark set at once, a step
    // after it: the mark is set in well under 100 ms (it never waits on a
    // step's doc write — seconds for a whole kit's edit), and lands after
    // the step before it and before the one after, holding the first.
    let dir = tempfile::tempdir().expect("a temp dir");
    let daw = Standalone::new();
    seed_song(&daw, "marked-song");
    saved(&daw, "marked-song", &dir.path().join("Marked.session"));
    set_verb(&daw, "marked-song", 0.5);
    super::record("marked-song", "Louder", Some("TEST")).expect("read");
    let began = Instant::now();
    let mark = super::mark("marked-song", "TEST", |_| "Before quieter".to_owned()).expect("marked");
    let took = began.elapsed();
    set_verb(&daw, "marked-song", 0.25);
    super::record("marked-song", "Quieter", Some("TEST")).expect("read");
    assert!(super::written(Duration::from_secs(10)), "the writes finish");
    assert!(took < Duration::from_millis(100), "the mark took {took:?}");
    let (moments, _) = super::history("marked-song", (0, 20)).expect("its history");
    let labels: Vec<&str> = moments.iter().map(|(m, _)| m.label.as_str()).collect();
    let at = |label: &str| {
        labels
            .iter()
            .position(|l| *l == label)
            .unwrap_or_else(|| panic!("no {label:?} in {labels:?}"))
    };
    // Newest first.
    assert!(
        at("Quieter") < at(&mark) && at(&mark) < at("Louder"),
        "{labels:?}"
    );
    super::restore(
        "marked-song",
        session_format::store::MomentRef::Mark(mark.clone()),
    )
    .expect("restored");
    assert_eq!(
        verb_volume(&daw, "marked-song"),
        Some(0.5),
        "the mark holds the step before it"
    );
}

/// A song opened as an alternative (an arrangement: `Song.session/Live`)
/// and saved into its own file, as SESSION_SAVE and a chart's rebuild do
/// (the file named, not the branch), saves to the alternative it is: the
/// song as recorded (the file's main branch) is left as it was.
#[test]
fn an_alternative_saved_into_its_own_file_saves_to_itself() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let daw = Standalone::new();
    seed_song(&daw, "recorded-song");
    set_verb(&daw, "recorded-song", 0.5);
    let file = dir.path().join("Song.session");
    saved(&daw, "recorded-song", &file);
    super::fork(&daw, "recorded-song", &file, "Live").expect("forked");
    let live = super::load(&daw, "Live", &file, "Live").expect("the alternative opens");
    set_verb(&daw, &live, 0.25);
    super::record(&live, "Quieter", None).expect("read");
    crate::session_file::save_session(&daw, &live, &file).expect("saved");
    let volume = |path: &Path| {
        SessionDoc::from_loro(super::doc_of(path).expect("it opens"))
            .read()
            .tracks
            .iter()
            .find(|t| t.name == "Verb")
            .map(|t| t.volume)
    };
    assert_eq!(
        volume(&file.join("Live")),
        Some(0.25),
        "the alternative holds its edit"
    );
    assert_eq!(
        volume(&file),
        Some(0.5),
        "the song as recorded is as it was"
    );
}

/// A song an older build wrote: a deleted item's map left whole, a mix
/// snapshot with its plugin state inline. Opened, it is tidied once.
#[test]
fn an_older_song_is_tidied_once_as_it_opens() {
    use engine_sync::model::{ItemState, SessionModel};
    use session_format::store::Store;

    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("Old.session");
    let state = "UExVR0lO".repeat(500);
    {
        let store = Store::open(&path).expect("a new file");
        let doc = SessionDoc::new();
        let mut model = SessionModel::default();
        for i in 0..3 {
            model.items.insert(
                format!("item-{i}"),
                ItemState {
                    track: "kick".into(),
                    length: 1.0,
                    ..ItemState::default()
                },
            );
        }
        model.project.ext.insert(
            crate::mix_states::SECTION.to_owned(),
            [(
                "snapshot:A".to_owned(),
                format!(r#"{{"fx":[{{"guid":"a","state":"{state}"}}]}}"#),
            )]
            .into(),
        );
        doc.write(&model, engine_sync::ORIGIN_LOCAL)
            .expect("written");
        // Deleted the old way: the key only.
        doc.loro()
            .get_map("items")
            .delete("item-2")
            .expect("deleted");
        doc.loro().commit();
        store.compact(MAIN, doc.loro()).expect("saved");
    }

    let (store, loaded, _) = super::open_branch(&path, MAIN).expect("opened");
    assert_eq!(
        engine_sync::doc::clear_deleted_entities(&loaded).expect("looked"),
        0,
        "the deleted item is cleared"
    );
    let ext = SessionDoc::from_loro(loaded.clone()).read().project.ext;
    let mix = &ext[crate::mix_states::SECTION];
    let hash = crate::mix_states::hash(&state);
    assert_eq!(mix[&crate::mix_states::state_key(&hash)], state);
    assert!(mix["snapshot:A"].contains(&format!("\"@{hash}\"")));
    let steps = store
        .lock()
        .expect("the store")
        .steps(MAIN)
        .expect("steps")
        .len();
    drop(store);

    let (store, again, _) = super::open_branch(&path, MAIN).expect("opened again");
    assert_eq!(again.get_deep_value(), loaded.get_deep_value());
    assert_eq!(
        store
            .lock()
            .expect("the store")
            .steps(MAIN)
            .expect("steps")
            .len(),
        steps,
        "tidied once"
    );
}

/// A real song tidied and folded, measured: run by hand on a copy, never in
/// CI. `STUDIO_FOLD_SONG` names its `.session`; the file is changed (the
/// tidy's step written, its history before the open archived beside it and
/// the branch made shallow), so back the song up first.
///
/// `cargo nextest run -p engine-core --run-ignored only -E
/// 'test(a_real_song_tidied_and_folded)' --no-capture`
#[test]
#[ignore = "a measurement on a real song, by hand"]
fn a_real_song_tidied_and_folded() {
    use session_format::store::Store;

    let Some(path) = std::env::var_os("STUDIO_FOLD_SONG") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    // The tidy's and the fold's spans, with what they record, as each closes.
    let _spans = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
            .with_env_filter("engine_core::project_file=info,session_format::store=info")
            .with_test_writer()
            .finish(),
    );
    let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let file_before = size(&path);

    let opened = Instant::now();
    let (store, doc, uncompacted) = super::open_branch(&path, MAIN).expect("opened, tidied");
    let open_ms = opened.elapsed().as_millis();
    let opened_at = doc.oplog_frontiers();
    let store = store.lock().expect("the store");
    let archive_path = store.archive_path(&path).expect("its archive's path");
    if let Some(folder) = archive_path.parent() {
        std::fs::create_dir_all(folder).expect("the archive's folder");
    }
    let archive = Store::open(&archive_path).expect("its archive");

    let folded = Instant::now();
    let taken = store
        .take_shallow(MAIN, &doc, &opened_at, &archive)
        .expect("a shallow snapshot");
    store
        .write_snapshot(taken.pack().expect("packed"))
        .expect("written");
    store.checkpoint().expect("checkpointed");
    archive.checkpoint().expect("archive checkpointed");
    let fold_ms = folded.elapsed().as_millis();
    let shallow_bytes = doc
        .export(engine_sync::loro::ExportMode::shallow_snapshot(&opened_at))
        .expect("exported")
        .len();
    drop(store);

    let reopened = Instant::now();
    let (_store, again, _) = super::open_branch(&path, MAIN).expect("reopened");
    let reopen_ms = reopened.elapsed().as_millis();
    assert_eq!(
        again.get_deep_value(),
        doc.get_deep_value(),
        "the song as it was"
    );

    let mb = |b: u64| format!("{}.{}", b / 1_000_000, b / 100_000 % 10);
    println!(
        "fold {}: file {} MB -> {} MB; shallow snapshot {} MB; \
         archive {} MB; {uncompacted} steps uncompacted; open+tidy {open_ms} ms, \
         fold {fold_ms} ms, reopen {reopen_ms} ms",
        path.display(),
        mb(file_before),
        mb(size(&path)),
        mb(u64::try_from(shallow_bytes).unwrap_or(u64::MAX)),
        mb(size(&archive_path)),
    );
}

#[test]
fn edits_from_elsewhere_are_named_as_they_were_made() {
    assert_eq!(super::remote_label(&[]), "Remote edits");
    assert_eq!(
        super::remote_label(&["Agent: tighten snare".to_owned()]),
        "Agent: tighten snare"
    );
    assert_eq!(
        super::remote_label(&["Agent: a".to_owned(), "Set volume".to_owned()]),
        "Agent: a; Set volume"
    );
}
