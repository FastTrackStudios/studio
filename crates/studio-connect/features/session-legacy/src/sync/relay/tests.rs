use std::time::Duration;

use crate::sync::model::{FxState, SessionModel, TrackState};
use crate::sync::relay::{Relay, Side, exchange, missing, same_lineage, seed};
use crate::sync::{ORIGIN_LOCAL, SessionDoc};

fn song() -> SessionModel {
    SessionModel {
        tracks: vec![TrackState {
            guid: "verb".into(),
            name: "Verb".into(),
            volume: 0.8,
            fx: vec![FxState {
                guid: "{VERB}".into(),
                plugin: "ReaVerb".into(),
                name: "ReaVerb".into(),
                enabled: true,
                ..FxState::default()
            }],
            ..TrackState::default()
        }],
        ..SessionModel::default()
    }
}

/// A song doc with history, and a live doc seeded from it.
fn seeded() -> (SessionDoc, SessionDoc) {
    let song_doc = SessionDoc::new();
    song_doc.write(&song(), ORIGIN_LOCAL).expect("written");
    let mut louder = song();
    louder.tracks[0].volume = 0.9;
    song_doc.write(&louder, ORIGIN_LOCAL).expect("an edit");
    let live = SessionDoc::new();
    live.loro()
        .import_batch(&seed(song_doc.loro()).expect("a seed"))
        .expect("seeded");
    (song_doc, live)
}

fn fx_count(doc: &SessionDoc) -> usize {
    doc.read().tracks.iter().map(|t| t.fx.len()).sum()
}

#[test]
fn a_seeded_doc_is_the_song_and_of_its_lineage() {
    let (song_doc, live) = seeded();
    assert_eq!(live.read(), song_doc.read());
    assert!(same_lineage(song_doc.loro(), live.loro()));
    assert!(same_lineage(live.loro(), song_doc.loro()));
    let apart = SessionDoc::new();
    apart.write(&song(), ORIGIN_LOCAL).expect("written");
    assert!(
        !same_lineage(song_doc.loro(), apart.loro()),
        "the same song written apart is another lineage"
    );
    assert!(!same_lineage(song_doc.loro(), SessionDoc::new().loro()));
    assert_eq!(missing(song_doc.loro(), live.loro()).expect("read"), None);
}

/// A fresh replica of a seeded doc (a peer joining the set later, its
/// version empty) gets the whole song from it, as a sync host sends it —
/// and stays of the song's lineage.
#[test]
fn a_fresh_replica_of_a_seeded_doc_gets_the_whole_song() {
    let (song_doc, live) = seeded();
    let fresh = SessionDoc::new();
    let backlog = live
        .loro()
        .export(loro::ExportMode::updates(&loro::VersionVector::default()))
        .expect("everything the seeded doc holds, from nothing");
    fresh.loro().import(&backlog).expect("taken whole");
    assert_eq!(fresh.read(), song_doc.read());
    assert!(same_lineage(song_doc.loro(), fresh.loro()));
}

/// The regression's shape at the doc level: edits on either side, passed
/// across many times over, leave one of each FX on both.
#[test]
fn edits_passed_back_and_forth_make_nothing_twice() {
    let (song_doc, live) = seeded();
    let a = Side::imported(song_doc.loro().clone());
    let b = Side::imported(live.loro().clone());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        for round in 0..40_u32 {
            let side = if round % 2 == 0 { &song_doc } else { &live };
            let mut model = side.read();
            model.tracks[0].volume = f64::from(round) / 40.0;
            side.write(&model, ORIGIN_LOCAL).expect("an edit");
            exchange(&a, &b).await.expect("passed");
            // Passed again, as a relay woken twice would: nothing moves.
            assert_eq!(exchange(&a, &b).await.expect("passed"), 0);
        }
    });
    assert_eq!(song_doc.read(), live.read());
    assert_eq!(fx_count(&song_doc), 1);
    assert_eq!(fx_count(&live), 1);
}

/// Wait (up to 5 s) until `what` holds.
async fn until(what: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !what() {
        assert!(std::time::Instant::now() < deadline, "never came");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// The relay passes what either side writes, and while locked passes
/// nothing until it is unlocked.
#[test]
fn a_relay_keeps_both_in_step_and_holds_while_locked() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        let (song_doc, live) = seeded();
        let (lock, locked) = tokio::sync::watch::channel(false);
        let relay = Relay::start(
            Side::imported(song_doc.loro().clone()),
            Side::imported(live.loro().clone()),
            locked,
        );
        let volume = |doc: &SessionDoc| doc.read().tracks[0].volume;

        let mut model = live.read();
        model.tracks[0].volume = 0.1;
        live.write(&model, ORIGIN_LOCAL).expect("an edit there");
        until(|| volume(&song_doc) == 0.1).await;

        lock.send_replace(true);
        let mut model = song_doc.read();
        model.tracks[0].volume = 0.2;
        song_doc.write(&model, ORIGIN_LOCAL).expect("an edit here");
        until(|| relay.held()).await;
        assert_eq!(volume(&live), 0.1, "held while locked");

        lock.send_replace(false);
        until(|| volume(&live) == 0.2).await;
        assert!(!relay.held());
        assert_eq!(fx_count(&live), 1);
    });
}

/// A set this device hosts (over iroh, here an in-process link) holds a
/// peer's edits back from the song while the device is locked, and the
/// song's edits back from the peer — the same as a set Task keeps — then
/// passes both ways when it is unlocked: the song's edits reach the peer
/// through the set's host, though they are imports there.
#[cfg(feature = "net")]
#[test]
fn a_hosted_set_holds_both_ways_while_locked() {
    use crate::sync::net::{SetHost, SetPeer, dial_fixed, song_id};
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        let (song_doc, _) = seeded();
        let set = SetHost::new(uuid::Uuid::from_u128(316));
        let doc_id = song_id("Set", "Song");
        let served = SessionDoc::new();
        served
            .loro()
            .import_batch(&seed(song_doc.loro()).expect("a seed"))
            .expect("seeded");
        let served_side = set.add_relayed(doc_id, &served);
        let server = architect::LocalServer::serve(
            set.mount(architect::LayerRouter::new()),
            architect::Scope::new(),
        );
        let client: crdt::sync::DocSyncClient = server.establish().await.expect("a client");
        let peer = SetPeer::sync_song(doc_id, dial_fixed(client));
        until(|| peer.read() == song_doc.read()).await;

        let (lock, locked) = tokio::sync::watch::channel(true);
        let relay = Relay::start(Side::imported(song_doc.loro().clone()), served_side, locked);
        let track = |doc: &SessionDoc| doc.read().tracks[0].clone();

        // Locked: the peer's edit reaches the set, not the song …
        let mut model = peer.read();
        model.tracks[0].volume = 0.1;
        peer.write(&model, ORIGIN_LOCAL).expect("the peer's edit");
        until(|| track(&served).volume == 0.1).await;
        // … and the song's edit stays here.
        let mut model = song_doc.read();
        model.tracks[0].name = "Hall".into();
        song_doc.write(&model, ORIGIN_LOCAL).expect("an edit here");
        until(|| relay.held()).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(track(&song_doc).volume, 0.9, "the peer's edit held");
        assert_eq!(track(&peer).name, "Verb", "this device's edit held");

        // Unlocked: both ways, the song's edit reaching the peer.
        lock.send_replace(false);
        until(|| track(&song_doc).volume == 0.1).await;
        until(|| track(&peer).name == "Hall").await;
        assert!(!relay.held());
        assert_eq!(fx_count(&peer), 1);
        drop(server);
    });
}
