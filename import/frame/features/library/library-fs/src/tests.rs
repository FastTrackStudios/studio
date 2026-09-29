use std::time::Instant;

use architect::{LocalServer, Scope, Services as _};

use super::*;

const PANEL: &str = r##"<Document><Page><Frame name="Panel" w=200 h=100 fill="#101010"><Frame name="Gain" w=60 h=60 control={{"binding":{"curve":"Linear","max":1.0,"min":0.0,"source":{"Param":{"addr":"amp/gain"}}},"kind":{"Knob":{"sweep_degrees":270.0}},"slots":[]}} /></Frame></Page></Document>"##;

/// A scratch library folder, removed on drop.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("library-fs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn panel(name: &str) -> String {
    PANEL.replace("\"Panel\"", &format!("{name:?}"))
}

fn catalog_json() -> String {
    r#"{"id":"test.amp","version":1,"params":[{"addr":"amp/gain","name":"Gain","short_name":null,"group":null,"kind":"Continuous","range":{"min":0.0,"max":10.0,"scale":"Linear","unit":"None","steps":null},"default":0.5,"bipolar":false,"featured":false}],"actions":[],"streams":[],"collections":[]}"#.to_owned()
}

#[test]
fn scans_markup_bundles_and_catalogs() {
    let dir = Dir::new("scan");
    std::fs::create_dir_all(dir.0.join("amp")).unwrap();
    std::fs::write(dir.0.join("amp/head.fm"), panel("Amp Head")).unwrap();
    std::fs::write(dir.0.join("amp/amp.catalog.json"), catalog_json()).unwrap();
    let scene = Content::markup_text(panel("Bundled")).decode().unwrap();
    std::fs::write(dir.0.join("bundled.frame"), Content::frame(&scene, "Bundled One").unwrap().bytes).unwrap();
    std::fs::write(dir.0.join("notes.txt"), "not a surface").unwrap();

    let lib = FsLibrary::open(&dir.0).unwrap();
    let all = lib.list(ListQuery::default());
    let ids: Vec<&str> = all.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["amp/head", "bundled"]);
    let head = &all[0];
    assert_eq!(head.name, "Amp Head", "name from the top-level frame");
    assert_eq!(head.version, 1);
    assert_eq!(head.fits.params, ["amp/gain"]);
    assert_eq!(head.tags, ["amp", "test.amp"], "folder + the catalog it fits");
    assert_eq!(all[1].name, "Bundled One", "a bundle's manifest title");
    assert_eq!(lib.catalogs().len(), 1);

    let engine = Signature::of_catalog(&lib.catalogs()[0]);
    assert_eq!(lib.list(ListQuery { fits: Some(engine), ..ListQuery::default() }).len(), 2);
    assert!(lib.list(ListQuery { fits: Some(Signature::default()), ..ListQuery::default() }).is_empty());
    assert_eq!(lib.list(ListQuery { text: "head".into(), ..ListQuery::default() }).len(), 1);
}

#[test]
fn publish_list_get_round_trip() {
    let dir = Dir::new("publish");
    let lib = FsLibrary::open(&dir.0).unwrap();
    let content = Content::markup_text(panel("Mine"));
    let meta = PublishMeta {
        name: "My Surface".into(),
        author: "cody".into(),
        tags: vec!["guitar".into()],
        license: "MIT".into(),
        ..PublishMeta::default()
    };
    let p = lib.publish(content.clone(), meta.clone()).unwrap();
    assert_eq!(p, Published { id: "my-surface".into(), version: 1 });
    assert!(dir.0.join("my-surface.fm").exists());
    assert!(dir.0.join("my-surface.meta.json").exists());

    let listed = lib.list(ListQuery { tags: vec!["guitar".into()], ..ListQuery::default() });
    assert_eq!(listed.len(), 1);
    assert_eq!((listed[0].author.as_str(), listed[0].license.as_str()), ("cody", "MIT"));
    let got = lib.get("my-surface".into(), None).unwrap();
    assert_eq!(got.content, content);
    assert_eq!(got.meta.fits.params, ["amp/gain"]);

    // Same name again → a fresh id; same id → a new version.
    assert_eq!(lib.publish(content.clone(), meta.clone()).unwrap().id, "my-surface-2");
    let v2 = Content::markup_text(panel("Mine v2"));
    let p2 = lib.publish(v2.clone(), PublishMeta { id: "my-surface".into(), ..meta }).unwrap();
    assert_eq!(p2.version, 2);
    assert_eq!(lib.get("my-surface".into(), None).unwrap().content, v2);
    assert_eq!(lib.get("my-surface".into(), Some(1)).unwrap().content, content, "old versions stay fetchable");
    assert!(matches!(lib.get("my-surface".into(), Some(9)), Err(LibraryError::NotFound(_))));

    // A publish writes the file, but a rescan doesn't count it twice.
    assert_eq!(lib.rescan(), 0);
    // A reopened library picks the version up from the sidecar.
    assert_eq!(FsLibrary::open(&dir.0).unwrap().get("my-surface".into(), None).unwrap().meta.version, 2);
}

#[test]
fn publish_rejects_bad_content_and_ids() {
    let dir = Dir::new("reject");
    let lib = FsLibrary::open(&dir.0).unwrap();
    let bad = lib.publish(Content::markup_text("<Document><Page>"), PublishMeta::default());
    assert!(matches!(bad, Err(LibraryError::BadContent(_))), "{bad:?}");
    for id in ["../escape", "/abs", "a//b", ".hidden"] {
        let r = lib.publish(Content::markup_text(panel("x")), PublishMeta { id: id.into(), ..PublishMeta::default() });
        assert!(matches!(r, Err(LibraryError::BadId(_))), "{id}: {r:?}");
    }
    assert!(lib.list(ListQuery::default()).is_empty());
}

#[test]
fn edits_on_disk_become_versions() {
    let dir = Dir::new("edits");
    let path = dir.0.join("s.fm");
    std::fs::write(&path, panel("One")).unwrap();
    let lib = FsLibrary::open(&dir.0).unwrap();
    assert_eq!(lib.rescan(), 0, "nothing changed");
    std::fs::write(&path, panel("Two")).unwrap();
    assert_eq!(lib.rescan(), 1);
    let s = lib.get("s".into(), None).unwrap();
    assert_eq!((s.meta.version, s.meta.name.as_str()), (2, "Two"));

    // A broken save still goes out (hosts reject it) and keeps the last
    // good metadata; the fix is the next version.
    std::fs::write(&path, "<Document>\n<Page>").unwrap();
    lib.rescan();
    let s = lib.get("s".into(), None).unwrap();
    assert_eq!((s.meta.version, s.meta.name.as_str()), (3, "Two"));
    assert!(s.content.decode().is_err());

    std::fs::remove_file(&path).unwrap();
    assert_eq!(lib.rescan(), 1);
    assert!(lib.list(ListQuery::default()).is_empty());
}

/// Wait for the next update on `rx`, up to 5 s.
async fn next<T: facet::Facet<'static>>(rx: &mut architect::vox::Rx<T>) -> T {
    let item = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await.expect("no update within 5 s");
    let item = item.expect("stream error").expect("stream ended");
    let mut out = None;
    let _ = item.map(|v| out = Some(v));
    out.expect("SelfRef::map runs its closure")
}

#[tokio::test]
async fn over_vox_with_a_watched_folder() {
    let dir = Dir::new("vox");
    let path = dir.0.join("live.fm");
    std::fs::write(&path, panel("Live")).unwrap();
    let lib = FsLibrary::open(&dir.0).unwrap();
    let _watch = lib.watch_every(Duration::from_millis(20));

    let scope = Scope::new();
    let local = LocalServer::serve(lib.clone().into_router(), scope.clone());
    let client: SurfaceStoreClient = local.establish().await.unwrap();
    let stream: SurfaceStoreStreamClient = local.establish().await.unwrap();

    // publish → list → get over the wire.
    let content = Content::markup_text(panel("Remote"));
    let p = client.publish(content.clone(), PublishMeta { id: "remote/one".into(), ..PublishMeta::default() }).await.unwrap();
    assert_eq!(p, Published { id: "remote/one".into(), version: 1 });
    let ids: Vec<String> = client.list(ListQuery::default()).await.unwrap().into_iter().map(|m| m.id).collect();
    assert_eq!(ids, ["live", "remote/one"]);
    assert_eq!(client.get("remote/one".into(), None).await.unwrap().content, content);
    let missing = client.get("nope".into(), None).await.unwrap_err();
    assert!(format!("{missing:?}").contains("NotFound"), "{missing:?}");

    // subscribe: the snapshot first, then an edit on disk arrives.
    let (tx, mut rx) = architect::vox::channel::<SurfaceUpdate>();
    let sub = stream.clone();
    let call = tokio::spawn(async move { sub.updates("live".into(), tx).await });
    let first = next(&mut rx).await;
    assert_eq!((first.id.as_str(), first.version), ("live", 1));
    let started = Instant::now();
    std::fs::write(&path, panel("Edited")).unwrap();
    let second = next(&mut rx).await;
    assert_eq!(second.version, 2);
    let scene = second.surface.unwrap().content.decode().unwrap();
    assert_eq!(scene_name(&scene).as_deref(), Some("Edited"));
    assert!(started.elapsed() < Duration::from_secs(2), "pushed promptly: {:?}", started.elapsed());

    // A publish to the same id is pushed too.
    client.publish(Content::markup_text(panel("Published")), PublishMeta { id: "live".into(), ..PublishMeta::default() }).await.unwrap();
    assert_eq!(next(&mut rx).await.version, 3);

    call.abort();
    scope.close().await;
}

#[tokio::test]
async fn subscribing_to_a_missing_surface_says_so() {
    let dir = Dir::new("missing");
    let lib = FsLibrary::open(&dir.0).unwrap();
    let scope = Scope::new();
    let local = LocalServer::serve(lib.clone().into_router(), scope.clone());
    let stream: SurfaceStoreStreamClient = local.establish().await.unwrap();
    let (tx, mut rx) = architect::vox::channel::<SurfaceUpdate>();
    let call = tokio::spawn(async move { stream.updates("later".into(), tx).await });
    let first = next(&mut rx).await;
    assert!(first.surface.is_none());
    // …and when it appears, it arrives.
    lib.publish(Content::markup_text(panel("Later")), PublishMeta { id: "later".into(), ..PublishMeta::default() }).unwrap();
    assert!(next(&mut rx).await.surface.is_some());
    call.abort();
    scope.close().await;
}
