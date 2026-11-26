//! Sync after sign-in, for any org (docs/design/sync.md).
//!
//! Signed in, a process keeps one [`DocsDriver`] per org that owns
//! something on this device — the person's own org for their rigs, an
//! org like FastTrackAudio for the projects it owns — each with its own
//! device index, all on the one kept session ([`crate::signed_in`]).
//! [`ensure`] makes an org's driver the first time it is asked for and
//! starts its connection loop; [`synced`] finds it again.
//!
//! Every synced doc on a device is a `.session` file ([`Files`]): placed
//! where the app keeps it (a rig's book beside its library, a project's
//! own file), anything else made on another device under the driver's
//! `docs/` directory.
//!
//! The connection is a loop: dial the org's lane, run a session until it
//! drops, wait, dial again — every session is a delta. Offline, everything
//! edits locally and catches up later. Signed out, the loop stops and the
//! files keep the docs themselves.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use crdt::PersistError;
use crdt::Persistence;
use crdt::docs::{DocMeta, DocStorage, DocsDriver, DocsLane, TursoDocsIndex};
use session_format::store::{MAIN, Replica, Store};
use uuid::Uuid;

/// How long a start waits for Task to name the personal org before it
/// goes on with the one it remembers.
const ASK_ORG: Duration = Duration::from_secs(5);

/// The longest wait between two tries to reach Task.
const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// An org on the account's Task server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Org {
    pub slug: String,
    pub id: Uuid,
}

/// `https://task.example` → `wss://task.example`.
#[must_use]
pub fn ws_base(server: &str) -> String {
    server
        .trim_end_matches('/')
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1)
}

/// The person's own org: asked of Task (made there on first ask), else
/// the one remembered from the last time it answered (`sync-org`), so a
/// start with no network still syncs into the right space later.
pub async fn personal_org(base: &str, token: &str) -> Option<Org> {
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
            remember("sync-org", &org);
            Some(org)
        }
        failed => {
            if let Ok(Err(e)) = failed {
                tracing::info!(
                    sync.outcome = "org-unanswered",
                    "sync: Task did not name the personal org: {e}"
                );
            }
            remembered("sync-org")
        }
    }
}

/// The org `slug` on `server` (an `https://` base), by the server's own
/// listing (`/.well-known/task-server.json`), else as last remembered.
pub async fn org(server: &str, slug: &str) -> Option<Org> {
    let key = format!("sync-org:{slug}");
    let url = format!(
        "{}/.well-known/task-server.json",
        server.trim_end_matches('/')
    );
    let listed = async {
        let body = reqwest::get(&url).await.ok()?.text().await.ok()?;
        org_in_listing(&body, slug)
    };
    match architect::platform::timeout(ASK_ORG, listed).await {
        Ok(Some(org)) => {
            remember(&key, &org);
            Some(org)
        }
        _ => remembered(&key),
    }
}

/// `slug`'s entry in a Task server's well-known listing.
fn org_in_listing(body: &str, slug: &str) -> Option<Org> {
    let listing: serde_json::Value = serde_json::from_str(body).ok()?;
    let entry = listing
        .get("orgs")?
        .as_array()?
        .iter()
        .find(|o| o.get("slug").and_then(serde_json::Value::as_str) == Some(slug))?;
    Some(Org {
        slug: slug.to_owned(),
        id: Uuid::parse_str(entry.get("id")?.as_str()?).ok()?,
    })
}

fn remember(key: &str, org: &Org) {
    utils::prefs::set_raw(key, &format!("{} {}", org.slug, org.id));
}

fn remembered(key: &str) -> Option<Org> {
    let kept = utils::prefs::get_raw(key)?;
    let (slug, id) = kept.split_once(' ')?;
    Some(Org {
        slug: slug.to_owned(),
        id: Uuid::parse_str(id).ok()?,
    })
}

// ── One driver per org ──────────────────────────────────────────────────────

/// An org's driver in this process, and where its docs' files are.
#[derive(Clone)]
pub struct Synced {
    pub org: Org,
    pub driver: DocsDriver,
    pub files: Files,
}

/// The drivers this process has made, by org slug.
fn drivers() -> &'static Mutex<HashMap<String, Synced>> {
    static DRIVERS: std::sync::LazyLock<Mutex<HashMap<String, Synced>>> =
        std::sync::LazyLock::new(Default::default);
    &DRIVERS
}

/// `slug`'s driver, if this process has made it.
#[must_use]
pub fn synced(slug: &str) -> Option<Synced> {
    drivers()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(slug)
        .cloned()
}

/// `org`'s driver in this process: made now (its device index at `index`,
/// docs from other devices under `elsewhere`) and connected for the life of
/// the process, or the one already made. Signed out, `None`: nothing syncs.
///
/// Two processes on one device must not share an `index` (each opens it
/// for writing): name it for the process's role, `<org>.index` beside
/// `<org>.projects.index`.
pub async fn ensure(org: Org, index: &Path, elsewhere: PathBuf) -> Option<Synced> {
    if let Some(made) = synced(&org.slug) {
        return Some(made);
    }
    let account = crate::signed_in()?;
    if let Some(dir) = index.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        tracing::warn!(sync.outcome = "no-dir", sync.org = %org.slug, "sync: no place for its index: {e}");
        return None;
    }
    let opened = match TursoDocsIndex::open_path(&index.to_string_lossy()).await {
        Ok(opened) => opened,
        Err(e) => {
            tracing::warn!(
                sync.outcome = "no-index",
                sync.org = %org.slug,
                "sync: the device index would not open: {e:?}"
            );
            return None;
        }
    };
    let files = Files::new(elsewhere);
    let made = Synced {
        org: org.clone(),
        driver: DocsDriver::new(opened, files.clone()),
        files,
    };
    // Two first asks at once: the first one in keeps its driver, and only
    // it connects.
    let (kept, first) = {
        let mut all = drivers().lock().unwrap_or_else(PoisonError::into_inner);
        match all.get(&org.slug) {
            Some(already) => (already.clone(), false),
            None => {
                all.insert(org.slug.clone(), made.clone());
                (made.clone(), true)
            }
        }
    };
    if first {
        tracing::info!(sync.org = %org.slug, "sync: on");
        architect::platform::spawn(stay_connected(
            made.driver.clone(),
            ws_base(&account.server),
            org,
        ));
    }
    Some(kept)
}

/// Run sessions with `org`'s lane, for the life of the process.
async fn stay_connected(driver: DocsDriver, base: String, org: Org) {
    let mut wait = Duration::from_secs(2);
    loop {
        // The token as it is now: a sign-in elsewhere may have renewed it.
        let Some(account) = crate::signed_in() else {
            tracing::info!(sync.outcome = "signed-out", sync.org = %org.slug, "sync: signed out — stopped");
            return;
        };
        let url = format!("{base}/org/{}/vox", org.slug);
        match lane(&url, &account.session.token).await {
            Ok(lane) => {
                wait = Duration::from_secs(2);
                if let Err(e) = driver.run(&lane).await {
                    tracing::info!(sync.outcome = "dropped", sync.org = %org.slug, "sync: the session ended: {e:?}");
                }
            }
            Err(e) => {
                tracing::info!(sync.outcome = "unreachable", sync.org = %org.slug, "sync: Task not reached: {e}");
            }
        }
        architect::platform::sleep(wait).await;
        wait = (wait * 2).min(MAX_BACKOFF);
    }
}

/// `org`'s lane URL on the account's server.
#[must_use]
pub fn org_url(server: &str, org: &str) -> String {
    format!("{}/org/{org}/vox", ws_base(server))
}

/// The three clients a session runs on, over one connection to `url`.
///
/// # Errors
///
/// The dial or a handshake failing, as a message.
pub async fn lane(url: &str, token: &str) -> Result<DocsLane, String> {
    Ok(DocsLane {
        docs: task_dial::establish_at(url, Some(token)).await?,
        changes: task_dial::establish_at(url, Some(token)).await?,
        sync: task_dial::establish_at(url, Some(token)).await?,
    })
}

// ── Where each doc's bytes live ─────────────────────────────────────────────

/// Every synced doc on this device is a `.session` file: placed where the
/// app keeps it, anything else made on another device under `elsewhere`.
#[derive(Clone)]
pub struct Files {
    placed: Arc<Mutex<HashMap<Uuid, PathBuf>>>,
    /// Each open doc's replica, so the app can name the steps it commits
    /// ([`Replica::name_next`]).
    replicas: Arc<Mutex<HashMap<Uuid, std::sync::Weak<Replica>>>>,
    elsewhere: PathBuf,
}

impl Files {
    #[must_use]
    pub fn new(elsewhere: PathBuf) -> Self {
        Self {
            placed: Arc::new(Mutex::new(HashMap::new())),
            replicas: Arc::new(Mutex::new(HashMap::new())),
            elsewhere,
        }
    }

    /// The replica `id`'s doc persists through, while the doc is open.
    #[must_use]
    pub fn replica(&self, id: Uuid) -> Option<Arc<Replica>> {
        self.replicas
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&id)
            .and_then(std::sync::Weak::upgrade)
    }

    /// `id`'s file is `file` (a rig's book beside its library, a project's
    /// own file). Ask before the driver opens the doc.
    pub fn place(&self, id: Uuid, file: PathBuf) {
        self.placed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, file);
    }

    /// Where `id`'s doc is kept on this device.
    #[must_use]
    pub fn file_for(&self, id: Uuid) -> PathBuf {
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
        // A file that has no branch for this doc yet: its main branch is
        // it — a book used signed out keeps its history under the doc's id
        // from now on, and a doc arriving from another device lands there.
        if store.branch_of(meta.doc_id).map_err(backend)?.is_none() {
            store.adopt_doc_id(MAIN, meta.doc_id).map_err(backend)?;
        }
        let replica = Arc::new(Replica::new(store));
        self.replicas
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(meta.doc_id, Arc::downgrade(&replica));
        Ok(replica)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_base_keeps_the_host_and_switches_the_scheme() {
        assert_eq!(
            ws_base("https://task.fasttrackstudio.app/"),
            "wss://task.fasttrackstudio.app"
        );
        assert_eq!(ws_base("http://127.0.0.1:9090"), "ws://127.0.0.1:9090");
    }

    #[test]
    fn an_org_is_found_in_the_servers_listing_by_slug() {
        let listing = r#"{"orgs":[
            {"slug":"codywright","id":"b56c2513-fcf5-42da-9a2a-000000000001"},
            {"slug":"fasttrackaudio","id":"a6825e82-7b10-4f85-a49a-4b4eb4cca558"}
        ]}"#;
        assert_eq!(
            org_in_listing(listing, "fasttrackaudio"),
            Some(Org {
                slug: "fasttrackaudio".into(),
                id: Uuid::parse_str("a6825e82-7b10-4f85-a49a-4b4eb4cca558").unwrap(),
            })
        );
        assert_eq!(org_in_listing(listing, "nobody"), None);
        assert_eq!(org_in_listing("not json", "codywright"), None);
    }

    #[test]
    fn a_doc_not_placed_is_kept_under_elsewhere_by_its_id() {
        let files = Files::new(PathBuf::from("/sync/docs"));
        let (placed, loose) = (Uuid::from_u128(1), Uuid::from_u128(2));
        files.place(placed, PathBuf::from("/songs/Unbreakable.session"));
        assert_eq!(
            files.file_for(placed),
            PathBuf::from("/songs/Unbreakable.session")
        );
        assert_eq!(
            files.file_for(loose),
            PathBuf::from(format!("/sync/docs/{loose}.session"))
        );
    }
}
