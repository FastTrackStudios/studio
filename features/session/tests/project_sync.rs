//! A shared project syncs through the org that owns it: its song and each
//! alternative are `session` docs on the org's driver, kept in the
//! project's own file — and another device gets them, edits them, and the
//! first device's file holds the edit (docs/design/sync.md, "Projects").

use std::path::Path;
use std::time::Duration;

use architect::{LayerRouter, LocalServer, Scope};
use crdt::docs::{
    DocsClient, DocsDriver, DocsHost, DocsLane, DocsStreamClient, InMemoryDocsIndex,
    InMemoryDocsStore, docs_rpc_service_descriptor, serve, stream_layer,
};
use crdt::in_memory::InMemoryPersistence;
use crdt::sync::{DocSyncClient, DocSyncDispatcher, doc_sync_service_descriptor};
use crdt::{CrdtDoc, DocRegistry};
use fts_account::sync::{Files, Org, Synced};
use loro::{ExportMode, LoroDoc, LoroValue};
use session::project_sync::{SESSION_KIND, register};
use session_format::store::{MAIN, Store};
use uuid::Uuid;

/// Task's side, in process: the catalog and the doc sync an org serves.
async fn task() -> LocalServer {
    let bytes = InMemoryPersistence::new();
    let registry = DocRegistry::new(move |id| {
        let bytes = bytes.clone();
        Box::pin(async move { CrdtDoc::open(id, bytes).await })
    });
    let host = DocsHost::open(InMemoryDocsStore::new(), registry)
        .await
        .expect("a host");
    let router = LayerRouter::new()
        .with(docs_rpc_service_descriptor(), serve(host.clone()))
        .merge(stream_layer(host.clone()))
        .with(doc_sync_service_descriptor(), DocSyncDispatcher::new(host));
    LocalServer::serve(router, Scope::new())
}

/// One device's driver for the org, its docs from elsewhere under `dir`.
fn device(dir: &Path) -> Synced {
    let files = Files::new(dir.join("sync/docs"));
    Synced {
        org: Org {
            slug: "fasttrackaudio".into(),
            id: Uuid::from_u128(7),
        },
        driver: DocsDriver::new(InMemoryDocsIndex::new(), files.clone()),
        files,
    }
}

/// `driver` online with `task`, from now on.
async fn online(task: &LocalServer, driver: DocsDriver) {
    let lane = DocsLane {
        docs: task.establish::<DocsClient>().await.expect("docs"),
        changes: task.establish::<DocsStreamClient>().await.expect("changes"),
        sync: task.establish::<DocSyncClient>().await.expect("sync"),
    };
    tokio::spawn(async move {
        let _ = driver.run(&lane).await;
    });
}

async fn until(what: &str, mut ok: impl AsyncFnMut() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !ok().await {
        assert!(std::time::Instant::now() < deadline, "never: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn items(doc: &LoroDoc) -> LoroValue {
    doc.get_map("items").get_deep_value()
}

/// `doc` with `key` = `value` put in its `items`, as one commit; the
/// update that made it.
fn put(doc: &LoroDoc, key: &str, value: f64) -> Vec<u8> {
    let before = doc.oplog_vv();
    doc.get_map("items").insert(key, value).expect("an insert");
    doc.commit();
    doc.export(ExportMode::updates(&before)).expect("an export")
}

/// A project made on device A: the song with a kick, and an alternative
/// forked from it with a snare as well.
fn project(path: &Path) -> Store {
    let store = Store::open(path).expect("a project file");
    let song = store.load(MAIN).expect("the song");
    let update = put(&song, "kick", 1.0);
    store
        .append_update(MAIN, "Add kick", &update)
        .expect("kept");
    let alt = store.load(MAIN).expect("the song again");
    store.fork("Drum Edits", MAIN, &alt).expect("forked");
    let update = put(&alt, "snare", 2.0);
    store
        .append_update("Drum Edits", "Add snare", &update)
        .expect("kept");
    store
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shared_song_and_its_alternative_reach_another_device_and_an_edit_comes_back() {
    let task = task().await;
    let (a_dir, b_dir) = (
        tempfile::tempdir().expect("a"),
        tempfile::tempdir().expect("b"),
    );
    let path = a_dir.path().join("Unbreakable.session");
    let store = project(&path);

    let a = device(a_dir.path());
    let made = register(&a, &path, &store).await.expect("registered");
    let (song, _) = made.get(MAIN).expect("the song's doc").clone();
    let (alt, _) = made
        .get("Drum Edits")
        .expect("the alternative's doc")
        .clone();
    assert_eq!(
        register(&a, &path, &store)
            .await
            .expect("again")
            .get(MAIN)
            .map(|(id, _)| *id),
        Some(song),
        "registering again opens the same docs"
    );
    online(&task, a.driver.clone()).await;

    // Device B knows nothing of the project but its org.
    let b = device(b_dir.path());
    online(&task, b.driver.clone()).await;
    until("B lists the song and its alternative", async || {
        let docs = b.driver.docs().await.unwrap_or_default();
        let of = |id: Uuid| {
            docs.iter()
                .find(|d| d.meta.doc_id == id)
                .map(|d| d.meta.clone())
        };
        matches!(
            (of(song), of(alt)),
            (Some(s), Some(a)) if s.kind == SESSION_KIND && a.kind == SESSION_KIND
                && s.name == "Unbreakable" && a.name == "Unbreakable — Drum Edits"
                && a.parent == Some(song)
        )
    })
    .await;

    let on_b = b.driver.open(song).await.expect("B opens the song");
    let want: LoroValue = items(&store.load(MAIN).expect("A's song"));
    until("B's song holds A's", async || items(on_b.loro()) == want).await;
    let alt_on_b = b.driver.open(alt).await.expect("B opens the alternative");
    let want_alt = items(&store.load("Drum Edits").expect("A's alternative"));
    until("B's alternative holds A's", async || {
        items(alt_on_b.loro()) == want_alt
    })
    .await;

    // B edits the song; A's own project file has it, on the song's branch.
    put(on_b.loro(), "hat", 3.0);
    until("A's file holds B's edit", async || {
        Store::open(&path)
            .and_then(|s| s.load(MAIN))
            .map(|doc| doc.get_map("items").get("hat").is_some())
            .unwrap_or(false)
    })
    .await;
    assert!(
        Store::open(&path)
            .and_then(|s| s.load("Drum Edits"))
            .map(|doc| doc.get_map("items").get("hat").is_none())
            .expect("A's alternative"),
        "an edit to the song stays out of its alternative"
    );
}

/// A song whose items were deleted and made again, over and over, as months
/// of editing do: each pass a fresh set of item maps, the old ones kept in
/// the doc's state.
fn churned(path: &Path) -> session::sync::SessionModel {
    use session::sync::{ItemState, ORIGIN_LOCAL, SessionDoc, SessionModel};
    let store = Store::open(path).expect("a project file");
    let doc = SessionDoc::from_loro(store.load(MAIN).expect("the song"));
    let mut model = SessionModel::default();
    for pass in 0..20 {
        model.items = (0..200)
            .map(|i| {
                (
                    format!("item-{pass}-{i}"),
                    ItemState {
                        track: "kick".into(),
                        position: f64::from(i) * 0.5,
                        length: 0.25,
                        ..ItemState::default()
                    },
                )
            })
            .collect();
        let before = doc.version();
        doc.write(&model, ORIGIN_LOCAL).expect("a pass");
        doc.loro().commit();
        let update = doc.updates_since(&before).expect("its update");
        store.append_update(MAIN, "Edit", &update).expect("kept");
    }
    let alt = store.load(MAIN).expect("the song again");
    store.fork("Drum Edits", MAIN, &alt).expect("forked");
    model
}

#[test]
fn a_song_folded_fresh_holds_the_same_song_in_less_and_its_alternative_still_opens() {
    use session::sync::SessionDoc;
    let dir = tempfile::tempdir().expect("a dir");
    let path = dir.path().join("Unbreakable.session");
    let model = churned(&path);
    let store = Store::open(&path).expect("reopen");
    let ids_before = (
        store.doc_id(MAIN).expect("id"),
        store.doc_id("Drum Edits").expect("id"),
    );

    let folded = session::project_sync::fold_fresh(&store).expect("folded");

    let (_, before, after) = folded.iter().find(|(b, _, _)| b == MAIN).expect("the song");
    assert!(
        after * 3 < *before,
        "the dead maps are left behind: {before} -> {after} bytes"
    );
    let store = Store::open(&path).expect("reopen");
    for branch in [MAIN, "Drum Edits"] {
        let read = SessionDoc::from_loro(store.load(branch).expect(branch)).read();
        assert_eq!(read.items, model.items, "{branch} holds the same items");
    }
    assert_eq!(
        (
            store.doc_id(MAIN).expect("id"),
            store.doc_id("Drum Edits").expect("id")
        ),
        ids_before,
        "each branch keeps its doc id"
    );
}

/// An edit a live set relays into a song synced with its org reaches the
/// org at once — on the relay's own session, not the device driver's next
/// attach (which sends only commits made on the device) — and reaches
/// another device of the org from there.
#[tokio::test(flavor = "multi_thread")]
async fn a_relayed_edit_reaches_the_org_at_once() {
    let task = task().await;
    let (a_dir, b_dir) = (
        tempfile::tempdir().expect("a"),
        tempfile::tempdir().expect("b"),
    );
    let path = a_dir.path().join("Unbreakable.session");
    let store = project(&path);
    let a = device(a_dir.path());
    let made = register(&a, &path, &store).await.expect("registered");
    let (song, on_a) = made.get(MAIN).expect("the song's doc").clone();
    online(&task, a.driver.clone()).await;
    let b = device(b_dir.path());
    online(&task, b.driver.clone()).await;
    until("B lists the song", async || {
        b.driver
            .docs()
            .await
            .unwrap_or_default()
            .iter()
            .any(|d| d.meta.doc_id == song)
    })
    .await;
    let on_b = b.driver.open(song).await.expect("B opens the song");
    let want = items(on_a.loro());
    until("B's song holds A's", async || items(on_b.loro()) == want).await;

    // A live set's copy of the song (its lineage), edited there.
    let live = LoroDoc::new();
    live.import(
        &on_a
            .loro()
            .export(ExportMode::Snapshot)
            .expect("the song whole"),
    )
    .expect("a copy");
    let relayed = put(&live, "ride", 4.0);

    // Relayed into A's synced doc: imported there, not committed — and
    // still at B, through Task, within moments.
    let server = std::sync::Arc::new(task);
    let dial: session::project_sync::Dial = {
        let server = std::sync::Arc::clone(&server);
        std::sync::Arc::new(move || {
            let server = std::sync::Arc::clone(&server);
            Box::pin(async move {
                server
                    .establish::<DocSyncClient>()
                    .await
                    .map_err(|e| format!("{e:?}"))
            })
        })
    };
    let side = session::project_sync::relayed_side(&on_a, song, dial);
    (side.take)(relayed).await.expect("taken");
    assert!(
        on_a.loro().get_map("items").get("ride").is_some(),
        "A holds it"
    );
    until("B has the relayed edit", async || {
        on_b.loro().get_map("items").get("ride").is_some()
    })
    .await;
    until("A's file holds it", async || {
        Store::open(&path)
            .and_then(|s| s.load(MAIN))
            .map(|doc| doc.get_map("items").get("ride").is_some())
            .unwrap_or(false)
    })
    .await;
}
