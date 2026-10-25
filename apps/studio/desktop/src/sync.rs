//! Sync after sign-in: the rig books, kept in step with the account's
//! other devices through Task (docs/design/sync.md, ADR 0002).
//!
//! Signed in, the engine runs one [`DocsDriver`] for the person's own
//! space — their personal org on Task — before its rigs start, and hands
//! out every rig's book ([`signal_rig_host::book::set_provider`]): the doc
//! the driver keeps and syncs, with the same id on every device
//! ([`rig_doc_id`], so the iPad's guitar rig and the desk's are one doc),
//! persisted to the rig's own `<library>.session` through
//! [`Replica`] — so a library used signed out keeps its history when its
//! owner signs in. The driver's index lives beside the libraries
//! (`<config>/signal/sync/`).
//!
//! The connection is a loop: dial the personal org's lane, run a session
//! until it drops, wait, dial again — every session is a delta. Offline,
//! everything edits locally and catches up later. Signed out, nothing
//! here runs and the books keep their files themselves.
//!
//! Which org is the personal one is asked of Task once and remembered
//! (`sync-org`), so a start with no network still syncs into the right
//! space when it comes back.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use crdt::docs::{CreateDoc, DocMeta, DocStorage, DocsDriver, DocsLane, TursoDocsIndex};
use crdt::{CrdtDoc, PersistError, Persistence};
use session_format::store::{MAIN, Replica, Store};
use uuid::Uuid;

/// The kind a rig library's doc has in the catalog.
pub const RIG_KIND: &str = "rig";

/// The namespace rig doc ids are made in: `<org id>/<library path>`.
const RIG_NAMESPACE: Uuid = Uuid::from_u128(0x6a0f_5d1e_2b4c_4f3a_9e7d_51c0_0000_0001);

/// How long the engine waits for Task to name the personal org before it
/// starts on the one it remembers (or, never having known one, without
/// sync this run).
const ASK_ORG: Duration = Duration::from_secs(5);

/// The longest wait between two tries to reach Task.
const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// The person's own org: where their rigs sync.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Org {
    pub slug: String,
    pub id: Uuid,
}

/// The id of the rig library at `rel` (its path under the signal config
/// directory: `rig`, `bass`, `drums/perform`) in `org`: the same on every
/// device, so each device's book is one doc.
#[must_use]
pub fn rig_doc_id(org: Uuid, rel: &str) -> Uuid {
    Uuid::new_v5(&RIG_NAMESPACE, format!("{org}/{rel}").as_bytes())
}

/// Start sync for this process's rigs, signed in; nothing otherwise. Call
/// it from the engine's runtime, after `books::enable` and before the
/// first rig makes its home.
pub async fn start() {
    let Some(account) = fts_account::signed_in() else {
        return;
    };
    let base = ws_base(&account.server);
    let Some(org) = personal_org(&base, &account.session.token).await else {
        tracing::warn!(
            sync.outcome = "no-org",
            "sync: no personal org known yet — this run's books stay on the device"
        );
        return;
    };
    let config = signal_rig_host::store::signal_config_dir();
    let state = config.join("sync");
    if let Err(e) = std::fs::create_dir_all(&state) {
        tracing::warn!(sync.outcome = "no-dir", "sync: no place for its index: {e}");
        return;
    }
    let index_path = state.join(format!("{}.index", org.slug));
    let index = match TursoDocsIndex::open_path(&index_path.to_string_lossy()).await {
        Ok(index) => index,
        Err(e) => {
            tracing::warn!(
                sync.outcome = "no-index",
                "sync: the device index would not open: {e:?}"
            );
            return;
        }
    };
    let files = Files::new(state.join("docs"));
    let driver = DocsDriver::new(index, files.clone());
    let handle = tokio::runtime::Handle::current();
    signal_rig_host::book::set_provider(Arc::new({
        let driver = driver.clone();
        let org = org.clone();
        move |dir: &Path| rig_book(&handle, &driver, &files, &org, &config, dir)
    }));
    tracing::info!(sync.org = %org.slug, "sync: on");
    architect::platform::spawn(stay_connected(driver, base, org));
}

/// The book for the rig library at `dir`: its doc, made or opened in the
/// driver (off the caller's thread: a rig makes its home from wherever it
/// is, async or not).
fn rig_book(
    handle: &tokio::runtime::Handle,
    driver: &DocsDriver,
    files: &Files,
    org: &Org,
    config: &Path,
    dir: &Path,
) -> Option<Arc<signal_rig_host::book::RigBook>> {
    let rel = dir
        .strip_prefix(config)
        .ok()?
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    let id = rig_doc_id(org.id, &rel);
    let file = dir.with_extension("session");
    files.place(id, file.clone());
    let name = format!("{} rig", title(&rel));
    let doc: CrdtDoc = std::thread::scope(|s| {
        s.spawn(|| {
            handle.block_on(driver.create(CreateDoc {
                doc_id: id,
                kind: RIG_KIND.to_owned(),
                name,
                parent: None,
            }))
        })
        .join()
        .ok()
    })?
    .inspect_err(|e| tracing::warn!(sync.rig = %rel, "sync: the rig's doc would not open: {e:?}"))
    .ok()?;
    let loro = doc.loro().clone();
    tracing::info!(sync.rig = %rel, sync.doc = %id, "sync: the rig's book is its synced doc");
    Some(signal_rig_host::book::RigBook::over(
        dir,
        &file,
        loro,
        Box::new(doc),
    ))
}

/// `drums/perform` → `Drums`.
fn title(rel: &str) -> String {
    let first = rel.split('/').next().unwrap_or(rel);
    let mut chars = first.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().chain(chars).collect::<String>()
    })
}

/// Run sessions with the personal org's lane, for the life of the process.
async fn stay_connected(driver: DocsDriver, base: String, org: Org) {
    let mut wait = Duration::from_secs(2);
    loop {
        // The token as it is now: a sign-in elsewhere may have renewed it.
        let Some(account) = fts_account::signed_in() else {
            tracing::info!(sync.outcome = "signed-out", "sync: signed out — stopped");
            return;
        };
        let url = format!("{base}/org/{}/vox", org.slug);
        match lane(&url, &account.session.token).await {
            Ok(lane) => {
                wait = Duration::from_secs(2);
                if let Err(e) = driver.run(&lane).await {
                    tracing::info!(sync.outcome = "dropped", "sync: the session ended: {e:?}");
                }
            }
            Err(e) => tracing::info!(sync.outcome = "unreachable", "sync: Task not reached: {e}"),
        }
        architect::platform::sleep(wait).await;
        wait = (wait * 2).min(MAX_BACKOFF);
    }
}

/// The three clients a session runs on, over one connection to `url`.
async fn lane(url: &str, token: &str) -> Result<DocsLane, String> {
    Ok(DocsLane {
        docs: task_dial::establish_at(url, Some(token)).await?,
        changes: task_dial::establish_at(url, Some(token)).await?,
        sync: task_dial::establish_at(url, Some(token)).await?,
    })
}

/// The personal org: asked of Task (made there on first ask), else the
/// one remembered from the last time it answered.
async fn personal_org(base: &str, token: &str) -> Option<Org> {
    let asked = architect::platform::timeout(ASK_ORG, async {
        let client: org_proto::OrgManagementServiceClient =
            task_dial::establish_at(&format!("{base}/server/vox"), Some(token)).await?;
        client
            .ensure_personal_org(org_proto::PersonalOrgRequest {
                session_token: token.to_owned(),
            })
            .await
            .map_err(|e| format!("{e:?}"))
    })
    .await;
    match asked {
        Ok(Ok(manifest)) => {
            let org = Org {
                slug: manifest.slug,
                id: manifest.id,
            };
            utils::prefs::set_raw("sync-org", &format!("{} {}", org.slug, org.id));
            Some(org)
        }
        failed => {
            if let Ok(Err(e)) = failed {
                tracing::info!(
                    sync.outcome = "org-unanswered",
                    "sync: Task did not name the personal org: {e}"
                );
            }
            let kept = utils::prefs::get_raw("sync-org")?;
            let (slug, id) = kept.split_once(' ')?;
            Some(Org {
                slug: slug.to_owned(),
                id: Uuid::parse_str(id).ok()?,
            })
        }
    }
}

/// `https://task.example` → `wss://task.example`.
fn ws_base(server: &str) -> String {
    server
        .trim_end_matches('/')
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1)
}

// ── Where each doc's bytes live ─────────────────────────────────────────────

/// Every synced doc on this device is a `.session` file: a rig's beside
/// its library, anything else made on another device under `docs/`.
#[derive(Clone)]
struct Files {
    placed: Arc<Mutex<HashMap<Uuid, PathBuf>>>,
    elsewhere: PathBuf,
}

impl Files {
    fn new(elsewhere: PathBuf) -> Self {
        Self {
            placed: Arc::new(Mutex::new(HashMap::new())),
            elsewhere,
        }
    }

    /// `id`'s file is `file` (a rig's book, beside its library).
    fn place(&self, id: Uuid, file: PathBuf) {
        self.placed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, file);
    }

    fn file_for(&self, id: Uuid) -> PathBuf {
        self.placed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&id)
            .cloned()
            .unwrap_or_else(|| self.elsewhere.join(format!("{id}.session")))
    }
}

#[async_trait::async_trait]
impl DocStorage for Files {
    async fn persistence(&self, meta: &DocMeta) -> Result<Arc<dyn Persistence>, PersistError> {
        let backend = |e: session_format::store::StoreError| PersistError::Backend(e.to_string());
        let file = self.file_for(meta.doc_id);
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let store = Store::open(&file).map_err(backend)?;
        // The file's song branch is this doc: a book used signed out
        // keeps its history under the doc's id from now on.
        if store.branch_of(meta.doc_id).map_err(backend)?.is_none() {
            store.adopt_doc_id(MAIN, meta.doc_id).map_err(backend)?;
        }
        Ok(Arc::new(Replica::new(store)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use architect::{LayerRouter, LocalServer, Scope};
    use crdt::DocRegistry;
    use crdt::docs::{
        DocsClient, DocsHost, DocsStreamClient, InMemoryDocsIndex, InMemoryDocsStore,
        docs_rpc_service_descriptor, serve, stream_layer,
    };
    use crdt::in_memory::InMemoryPersistence;
    use crdt::sync::{DocSyncClient, DocSyncDispatcher, doc_sync_service_descriptor};
    use signal_rig_host::book::{BookRead, RigBook};

    #[derive(Clone, Debug, PartialEq, facet::Facet)]
    struct Sets {
        name: String,
        songs: Vec<String>,
    }

    /// Task's side, in process: the catalog and the doc sync a host serves.
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

    /// One device: its driver, online with `task`, and its guitar rig's
    /// book over the driver's doc, kept in `<dir>/rig.session`.
    async fn device(task: &LocalServer, dir: &Path, id: Uuid) -> Arc<RigBook> {
        let files = Files::new(dir.join("sync/docs"));
        let driver = DocsDriver::new(InMemoryDocsIndex::new(), files.clone());
        let library = dir.join("rig");
        files.place(id, library.with_extension("session"));
        let doc = driver
            .create(CreateDoc {
                doc_id: id,
                kind: RIG_KIND.into(),
                name: "Rig rig".into(),
                parent: None,
            })
            .await
            .expect("the rig's doc");
        let lane = DocsLane {
            docs: task.establish::<DocsClient>().await.expect("docs"),
            changes: task.establish::<DocsStreamClient>().await.expect("changes"),
            sync: task.establish::<DocSyncClient>().await.expect("sync"),
        };
        tokio::spawn(async move {
            let _ = driver.run(&lane).await;
        });
        let loro = doc.loro().clone();
        RigBook::over(
            &library,
            &library.with_extension("session"),
            loro,
            Box::new(doc),
        )
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_rig_edited_on_one_device_arrives_in_the_others_book_and_file() {
        let task = task().await;
        let (ipad, desk) = (
            tempfile::tempdir().expect("ipad"),
            tempfile::tempdir().expect("desk"),
        );
        let id = rig_doc_id(Uuid::from_u128(42), "rig");
        let on_ipad = device(&task, ipad.path(), id).await;
        let sets = Sets {
            name: "CAA 2026".into(),
            songs: vec!["12 to 12".into(), "1901".into()],
        };
        on_ipad
            .write("setlists.styx", &sets)
            .expect("the iPad's edit");

        let on_desk = device(&task, desk.path(), id).await;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if matches!(on_desk.read::<Sets>("setlists.styx"), BookRead::Ok(ref v) if *v == sets) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the desk never got the iPad's setlist"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            on_desk.take_changes().contains(&"setlists.styx".to_owned()),
            "the desk's rig is told, for its hot reload"
        );

        // And back: the desk reorders, the iPad has it.
        let reordered = Sets {
            songs: vec!["1901".into(), "12 to 12".into()],
            ..sets.clone()
        };
        on_desk
            .write("setlists.styx", &reordered)
            .expect("the desk's edit");
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !matches!(on_ipad.read::<Sets>("setlists.styx"), BookRead::Ok(ref v) if *v == reordered)
        {
            assert!(
                std::time::Instant::now() < deadline,
                "the iPad never got the desk's reorder"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // What arrived is in the desk's own `.session`, not only in memory.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let kept = Store::open(&desk.path().join("rig.session"))
                .and_then(|s| s.load(MAIN))
                .map(|doc| doc.get_map("library").get("setlists.styx").is_some())
                .unwrap_or(false);
            if kept {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the desk's file never held it"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    #[test]
    fn a_rig_is_one_doc_on_every_device_and_another_in_another_org() {
        let org = Uuid::from_u128(7);
        assert_eq!(rig_doc_id(org, "rig"), rig_doc_id(org, "rig"));
        assert_ne!(rig_doc_id(org, "rig"), rig_doc_id(org, "bass"));
        assert_ne!(
            rig_doc_id(org, "rig"),
            rig_doc_id(Uuid::from_u128(8), "rig")
        );
        assert_eq!(title("drums/perform"), "Drums");
        assert_eq!(
            ws_base("https://task.fasttrackstudio.app/"),
            "wss://task.fasttrackstudio.app"
        );
    }
}
