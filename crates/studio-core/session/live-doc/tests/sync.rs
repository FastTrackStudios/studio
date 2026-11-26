//! A doc shared over an in-process link: joiners seeded, kept in step both
//! ways, a big seed in pieces, a shallow host, a reconnect, presence.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use architect::{LayerRouter, LocalServer, Scope};
use live_doc::{
    DocPresenceClient, DocPresenceDispatcher, DocSyncClient, DocSyncDispatcher, DocSyncHost, Hosts,
    PresenceHost, PresencePeer, SyncedDoc, doc_presence_service_descriptor,
    doc_sync_service_descriptor,
};
use loro::{ExportMode, LoroDoc};
use uuid::Uuid;

const DOC: Uuid = Uuid::from_u128(0x5e55_1011);

fn serve(host: DocSyncHost) -> LocalServer {
    LocalServer::serve(
        LayerRouter::new().with(doc_sync_service_descriptor(), DocSyncDispatcher::new(host)),
        Scope::new(),
    )
}

/// A replica of `DOC`, running on its own task; it and the task.
async fn join(server: &LocalServer) -> (LoroDoc, tokio::task::JoinHandle<()>) {
    let client: DocSyncClient = server.establish().await.expect("the link");
    let doc = LoroDoc::new();
    let mut synced = SyncedDoc::new(DOC, doc.clone());
    let task = tokio::spawn(async move {
        let _ = synced.run(&client).await;
    });
    (doc, task)
}

/// Waits until `f` holds, or fails after five seconds.
async fn until(what: &str, f: impl Fn() -> bool) {
    for _ in 0..500 {
        if f() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("never: {what}");
}

fn title(doc: &LoroDoc) -> Option<String> {
    doc.get_map("song")
        .get("title")
        .and_then(|v| v.into_value().ok())
        .and_then(|v| v.into_string().ok())
        .map(|s| s.to_string())
}

#[tokio::test(flavor = "multi_thread")]
async fn two_joiners_and_the_host_end_up_with_one_doc() {
    let doc = LoroDoc::new();
    doc.get_map("song").insert("title", "Before").unwrap();
    doc.commit();
    let server = serve(DocSyncHost::new(DOC, doc.clone()));
    let (a, _ta) = join(&server).await;
    let (b, _tb) = join(&server).await;
    until("both joiners are seeded", || {
        title(&a).as_deref() == Some("Before") && title(&b).as_deref() == Some("Before")
    })
    .await;

    a.get_map("song").insert("title", "From A").unwrap();
    a.commit();
    until("A's edit reaches B and the host", || {
        title(&b).as_deref() == Some("From A") && title(&doc).as_deref() == Some("From A")
    })
    .await;

    doc.get_map("song").insert("key", "E").unwrap();
    doc.commit();
    until("the host's own edit reaches both", || {
        [&a, &b]
            .iter()
            .all(|d| d.get_map("song").get("key").is_some())
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_seed_over_a_frame_comes_in_pieces_and_imports_whole() {
    let doc = LoroDoc::new();
    let notes = doc.get_list("notes");
    for i in 0..2_000 {
        notes.push(format!("note {i} of a long song")).unwrap();
    }
    doc.commit();
    let seed = doc
        .export(ExportMode::shallow_snapshot(&doc.oplog_frontiers()))
        .unwrap();
    // Frames a fifth of the seed: four pieces ahead of the attach.
    let host = DocSyncHost::new(DOC, doc.clone()).with_max_frame(seed.len() / 5);
    let server = serve(host);
    let (joiner, _task) = join(&server).await;
    until("the joiner has every note", || {
        joiner.get_list("notes").len() == 2_000
    })
    .await;
    assert_eq!(joiner.get_deep_value(), doc.get_deep_value());
    assert!(
        joiner.is_shallow(),
        "a joiner is seeded with the state, not the history"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shallow_host_seeds_a_joiner_and_takes_its_edits() {
    let full = LoroDoc::new();
    for take in ["one", "two", "three"] {
        full.get_map("song").insert("title", take).unwrap();
        full.commit();
    }
    // The song as it is opened: folded shallow at its latest.
    let doc = LoroDoc::new();
    doc.import(
        &full
            .export(ExportMode::shallow_snapshot(&full.oplog_frontiers()))
            .unwrap(),
    )
    .unwrap();
    assert!(doc.is_shallow());

    let server = serve(DocSyncHost::new(DOC, doc.clone()));
    let (joiner, _task) = join(&server).await;
    until("the joiner is seeded", || {
        title(&joiner).as_deref() == Some("three")
    })
    .await;
    joiner.get_map("song").insert("title", "four").unwrap();
    joiner.commit();
    until("its edit reaches the host", || {
        title(&doc).as_deref() == Some("four")
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_replica_that_edited_while_away_sends_it_on_reconnect() {
    let doc = LoroDoc::new();
    doc.get_map("song").insert("title", "Start").unwrap();
    doc.commit();
    let server = serve(DocSyncHost::new(DOC, doc.clone()));

    let client: DocSyncClient = server.establish().await.unwrap();
    let replica = LoroDoc::new();
    let mut synced = SyncedDoc::new(DOC, replica.clone());
    let first = tokio::spawn(async move {
        let _ = synced.run(&client).await;
        synced
    });
    until("seeded", || title(&replica).as_deref() == Some("Start")).await;
    first.abort();
    let _ = first.await;

    // Away: an edit the host has not seen, and a new replica driver over
    // the same doc (its outbox did not survive), as after a restart.
    replica.get_map("song").insert("title", "Offline").unwrap();
    replica.commit();
    let mut again = SyncedDoc::new(DOC, replica.clone());
    let client: DocSyncClient = server.establish().await.unwrap();
    let _second = tokio::spawn(async move {
        let _ = again.run(&client).await;
    });
    until("the offline edit reaches the host", || {
        title(&doc).as_deref() == Some("Offline")
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn hosts_serve_each_doc_under_its_id_and_an_unknown_one_empty() {
    let hosts = Hosts::new(30_000);
    let song = LoroDoc::new();
    song.get_map("song").insert("title", "Set opener").unwrap();
    song.commit();
    let _ = hosts.add(DOC, song);
    let server = LocalServer::serve(
        LayerRouter::new().with(
            doc_sync_service_descriptor(),
            DocSyncDispatcher::new(hosts.clone()),
        ),
        Scope::new(),
    );
    let (joiner, _task) = join(&server).await;
    until("the song is served", || {
        title(&joiner).as_deref() == Some("Set opener")
    })
    .await;
    assert!(
        hosts.doc(Uuid::from_u128(7)).doc().oplog_vv().is_empty(),
        "an id it does not have is an empty doc"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn presence_goes_both_ways_and_a_peer_reads_the_hosts_clock() {
    let host = PresenceHost::new(DOC, 30_000).with_clock(std::sync::Arc::new(|| 1.0e9));
    let server = LocalServer::serve(
        LayerRouter::new().with(
            doc_presence_service_descriptor(),
            DocPresenceDispatcher::new(host.clone()),
        ),
        Scope::new(),
    );
    let mut peers = Vec::new();
    for name in ["ada", "bo"] {
        let client: DocPresenceClient = server.establish().await.unwrap();
        let (peer, mut driver) = PresencePeer::new(DOC, 30_000);
        peer.set(&format!("{name}/state"), name);
        tokio::spawn(async move {
            let _ = driver.run(&client).await;
        });
        peers.push(peer);
    }
    until("each sees the other", || {
        peers.iter().all(|p| p.states().len() == 2)
    })
    .await;
    until("a peer estimates the host's clock", || {
        peers[0]
            .clock()
            .now_ms()
            .is_some_and(|t| (t - 1.0e9).abs() < 1.0e6)
    })
    .await;
    peers[1].delete("bo/state");
    until("a withdrawn key goes everywhere", || {
        peers[0].states().len() == 1 && host.store().get("bo/state").is_none()
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_peer_seeds_an_empty_set_with_its_song_in_pieces_both_ways() {
    // The set has none of the song yet: the host's doc is empty, and a
    // second joiner is already attached to it.
    let host_doc = LoroDoc::new();
    let server = serve(DocSyncHost::new(DOC, host_doc.clone()).with_max_frame(512));
    let (other, _other) = join(&server).await;

    // This peer's song, folded shallow, seeds the set: its snapshot goes up
    // its replica's connection (as `relay::seed` passes it) in pieces.
    let song = LoroDoc::new();
    let notes = song.get_list("notes");
    for i in 0..2_000 {
        notes.push(format!("note {i} of a long song")).unwrap();
    }
    song.commit();
    let snapshot = song
        .export(ExportMode::shallow_snapshot(&song.oplog_frontiers()))
        .unwrap();
    assert!(snapshot.len() > 4 * 512, "the seed is several frames");

    let client: DocSyncClient = server.establish().await.unwrap();
    let replica = LoroDoc::new();
    let mut synced = SyncedDoc::new(DOC, replica.clone()).with_max_frame(512);
    let outbox = synced.outbox();
    let _task = tokio::spawn(async move {
        let _ = synced.run(&client).await;
    });
    replica.import(&snapshot).unwrap();
    outbox.send(snapshot).unwrap();

    until("the host and the other joiner have the song", || {
        host_doc.get_list("notes").len() == 2_000 && other.get_list("notes").len() == 2_000
    })
    .await;
    assert_eq!(other.get_deep_value(), song.get_deep_value());
}
