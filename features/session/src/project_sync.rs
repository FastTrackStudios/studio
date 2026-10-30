//! A project file synced through the org that owns it
//! (docs/design/sync.md, "Projects").
//!
//! A project names its owner in its own file (`sync:org` in the store's
//! meta), so wherever it is opened it syncs into the same place. Each
//! branch is a doc of kind [`SESSION_KIND`] on that org's driver — the
//! song as `Strict`, each alternative a doc whose parent is the branch it
//! was forked from — persisted to the project's own file through its
//! replica. Nothing is copied: the file stays the one place the project
//! lives, and sync is one more writer of it.
//!
//! While a synced song is open, the bridge edits the driver's doc (one
//! `LoroDoc` in memory), so a remote update and a local step land in the
//! same place: the doc's own writer appends every local commit, named by
//! [`SyncedBranch::name_next`], and what arrives from another device is
//! applied to the engine ([`watch_remote`]).
//!
//! Sharing starts each branch over from what it holds now ([`fold_fresh`]):
//! a song's live state keeps every map it ever deleted and made again, so
//! months of editing weigh far more than the song (Unbreakable: 157 MB of
//! state, 26 MB written fresh) — and a doc's first sync is one message,
//! under the link's 64 MB. The history before is kept beside the file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crdt::CrdtDoc;
use crdt::docs::{CreateDoc, DocSyncEvent, Lineage};
use crdt::sync::DocSyncClient;
use fts_account::sync::Synced;
use session_format::store::{MAIN, Replica, Store};
use uuid::Uuid;

/// The kind a project branch's doc has in its org's catalog.
pub const SESSION_KIND: &str = "session";

/// The store meta key naming the org a project syncs into.
const ORG_META: &str = "sync:org";

/// The store meta key set once the project's branches start fresh
/// ([`fold_fresh`]): sharing again does not fold again.
const FRESH_META: &str = "sync:fresh";

/// How long [`share`] waits for every branch to reach Task.
const SHARE_WAIT: Duration = Duration::from_secs(600);

/// The org `store`'s project syncs into, if it has been shared.
#[must_use]
pub fn org_of(store: &Store) -> Option<String> {
    store
        .meta(ORG_META)
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
}

/// Where this process keeps its project drivers' indexes and the docs
/// other devices made: beside Session's own data, apart from the rig
/// engine's index (two processes must not share one).
fn sync_dir() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("Session").join("sync"))
}

/// `slug`'s driver in this process, made on first ask. Signed out (or the
/// org unknown and never seen), `None`: the project stays on the device.
async fn driver(slug: &str) -> Option<Synced> {
    if let Some(made) = fts_account::sync::synced(slug) {
        return Some(made);
    }
    let account = fts_account::signed_in()?;
    let org = fts_account::sync::org(&account.server, slug).await?;
    let dir = sync_dir()?;
    fts_account::sync::ensure(
        org,
        &dir.join(format!("{slug}.projects.index")),
        dir.join("docs"),
    )
    .await
}

/// The song's name in the catalog: its file's stem, an alternative's
/// branch after it.
fn doc_name(path: &Path, branch: &str) -> String {
    let song = path
        .file_stem()
        .map_or_else(|| "Song".to_owned(), |s| s.to_string_lossy().into_owned());
    if branch == MAIN {
        song
    } else {
        format!("{song} — {branch}")
    }
}

/// The project's branches, each after the branch it was forked from.
fn parents_first(
    mut branches: Vec<session_format::store::Branch>,
) -> Vec<(String, Option<String>)> {
    let mut ordered: Vec<(String, Option<String>)> = Vec::with_capacity(branches.len());
    while !branches.is_empty() {
        let ready = branches.iter().position(|b| {
            b.parent
                .as_ref()
                .is_none_or(|p| ordered.iter().any(|(name, _)| name == p))
        });
        // A parent that is not in the file: take the branch as a root
        // rather than lose it.
        let b = branches.remove(ready.unwrap_or(0));
        ordered.push((b.name, b.parent));
    }
    ordered
}

/// Every branch of the project at `path` made a doc on `synced`'s driver
/// (idempotent: a branch already made is opened), placed in the project's
/// own file. Returns each branch's doc id and its doc.
///
/// # Errors
///
/// The file's branches would not read, or the driver refused a branch
/// (another kind or lineage already holds its id).
pub async fn register(
    synced: &Synced,
    path: &Path,
    store: &Store,
) -> Result<HashMap<String, (Uuid, CrdtDoc)>, String> {
    let branches = store.branches().map_err(|e| e.to_string())?;
    register_branches(synced, path, store, branches, |_| true, &HashMap::new()).await
}

/// [`register`] for the branches `wanted` says, parents first, given the
/// ids of those already made (`made`, the parents a wanted branch may
/// name). Returns what it made.
async fn register_branches(
    synced: &Synced,
    path: &Path,
    store: &Store,
    branches: Vec<session_format::store::Branch>,
    wanted: impl Fn(&str) -> bool,
    made: &HashMap<String, Uuid>,
) -> Result<HashMap<String, (Uuid, CrdtDoc)>, String> {
    let mut ids = made.clone();
    let mut out: HashMap<String, (Uuid, CrdtDoc)> = HashMap::new();
    for (branch, parent) in parents_first(branches) {
        if ids.contains_key(&branch) || !wanted(&branch) {
            continue;
        }
        let id = store.doc_id(&branch).map_err(|e| e.to_string())?;
        synced.files.place(id, path.to_path_buf());
        let doc = tracing::Instrument::instrument(
            synced.driver.create(CreateDoc {
                doc_id: id,
                kind: SESSION_KIND.to_owned(),
                name: doc_name(path, &branch),
                parent: parent.and_then(|p| ids.get(&p).copied()),
                lineage: Lineage::Strict,
            }),
            tracing::info_span!("project_sync.create_doc", sync.branch = %branch),
        )
        .await
        .map_err(|e| format!("{branch}: {e:?}"))?;
        ids.insert(branch.clone(), id);
        out.insert(branch, (id, doc));
    }
    Ok(out)
}

/// `branch` and every branch it descends from, by name.
fn lineage_of(branches: &[session_format::store::Branch], branch: &str) -> Vec<String> {
    let mut chain = vec![branch.to_owned()];
    let mut at = branch.to_owned();
    while let Some(parent) = branches
        .iter()
        .find(|b| b.name == at)
        .and_then(|b| b.parent.clone())
    {
        if chain.contains(&parent) {
            break;
        }
        chain.push(parent.clone());
        at = parent;
    }
    chain
}

/// The project's branches not registered yet ([`open`] makes the one it
/// opens and its ancestors), registered in the background: on a handle of
/// its own on the file, under a span of their own.
fn register_rest_later(synced: &Synced, path: &Path, made: &HashMap<String, (Uuid, CrdtDoc)>) {
    use tracing::Instrument as _;
    let (synced, path) = (synced.clone(), path.to_path_buf());
    let made: HashMap<String, Uuid> = made.iter().map(|(b, (id, _))| (b.clone(), *id)).collect();
    architect::platform::spawn(
        async move {
            let Ok(store) = Store::open(&path) else {
                return;
            };
            let Ok(branches) = store.branches() else {
                return;
            };
            let known = made.clone();
            if let Err(e) = register_branches(
                &synced,
                &path,
                &store,
                branches,
                |b| !known.contains_key(b),
                &made,
            )
            .await
            {
                tracing::warn!(why = %e, "a project branch would not register with its sync");
            }
        }
        .instrument(tracing::info_span!("project_sync.register_rest")),
    );
}

/// A song's branch as its org's synced doc.
pub struct SyncedBranch {
    pub id: Uuid,
    pub org: String,
    doc: CrdtDoc,
    replica: Option<Arc<Replica>>,
}

impl SyncedBranch {
    /// The doc the bridge edits.
    #[must_use]
    pub fn loro(&self) -> crate::sync::loro::LoroDoc {
        self.doc.loro().clone()
    }

    /// Record the next commit under `label` in the file's history.
    pub fn name_next(&self, label: &str) {
        if let Some(replica) = &self.replica {
            replica.name_next(label);
        }
    }

    /// This branch as a side of a [`crate::sync::relay::Relay`] (a live
    /// set's doc of the song kept in step with it): what it is passed is
    /// applied to the doc, written through its file, and sent to the org at
    /// once ([`relayed_side`], over a session of its own with Task). Signed
    /// out, it is written through the file only, and reaches the org with
    /// the next sync.
    #[must_use]
    pub fn side(&self) -> crate::sync::relay::Side {
        match fts_account::signed_in() {
            Some(account) => {
                let url = fts_account::sync::org_url(&account.server, &self.org);
                let token = account.session.token.clone();
                let dial: Dial = Arc::new(move || {
                    let (url, token) = (url.clone(), token.clone());
                    Box::pin(async move {
                        task_dial::establish_at::<DocSyncClient>(&url, Some(&token))
                            .await
                            .map_err(|e| format!("dialling {url}: {e}"))
                    })
                });
                relayed_side(&self.doc, self.id, dial)
            }
            None => {
                tracing::warn!(sync.org = %self.org, sync.doc = %self.id, "signed out: what a live set sends this song reaches its org with the next sync");
                durable_side(&self.doc, self.id, None)
            }
        }
    }
}

/// Makes a sync client for Task afresh: what an upstream session dials
/// again with when its connection goes.
pub type Dial = Arc<
    dyn Fn() -> architect::platform::BoxFuture<'static, Result<DocSyncClient, String>>
        + Send
        + Sync,
>;

/// The synced doc `doc` (id `id`) as a relay's side whose updates reach
/// the org at once.
///
/// The org's driver sends up only the commits made on this device: what is
/// imported (a live set's edit relayed in) would wait for its next attach.
/// So this side keeps a sync session of its own with Task for the doc,
/// dialled with `dial` (and again whenever it drops), and sends each update
/// it is passed up that session's outbox, after applying it to the doc and
/// writing it through the file. Task hands it on to every other session on
/// the doc — this device's driver gets it back and already has it — and
/// never back to the live set it came from: that set's doc already holds
/// it. The session ends when the last copy of the side is dropped.
#[must_use]
pub fn relayed_side(doc: &CrdtDoc, id: Uuid, dial: Dial) -> crate::sync::relay::Side {
    let mut session = crdt::sync::SyncedDoc::new(id, doc.clone());
    let outbox = session.outbox();
    let (stop, mut stopped) = tokio::sync::watch::channel(false);
    architect::platform::spawn(async move {
        loop {
            let attempt = async {
                match dial().await {
                    Ok(client) => {
                        if let Err(e) = session.run(&client).await {
                            tracing::debug!(sync.doc = %id, sync.error = %e, "the song's relay session to its org ended; dialling again");
                        }
                    }
                    Err(e) => {
                        tracing::debug!(sync.doc = %id, sync.error = %e, "the song's relay session to its org could not dial yet");
                    }
                }
                architect::platform::sleep(Duration::from_secs(2)).await;
            };
            tokio::select! {
                _ = stopped.changed() => break,
                () = attempt => {}
            }
        }
    });
    durable_side(doc, id, Some((outbox, Arc::new(Upstream(stop)))))
}

/// Ends a [`relayed_side`]'s session when its last copy drops.
struct Upstream(tokio::sync::watch::Sender<bool>);

impl Drop for Upstream {
    fn drop(&mut self) {
        let _ = self.0.send(true);
    }
}

/// `doc` as a relay's side: an update applied and written through its
/// file, then sent up `up`'s outbox when there is one.
fn durable_side(
    doc: &CrdtDoc,
    id: Uuid,
    up: Option<(tokio::sync::mpsc::UnboundedSender<Vec<u8>>, Arc<Upstream>)>,
) -> crate::sync::relay::Side {
    let into = doc.clone();
    crate::sync::relay::Side {
        doc: doc.loro().clone(),
        take: Arc::new(move |bytes: Vec<u8>| {
            let (into, up) = (into.clone(), up.clone());
            Box::pin(async move {
                into.apply_remote_durable(id, &bytes)
                    .await
                    .map_err(|e| eyre::eyre!("the synced song took no update: {e}"))?;
                if let Some((outbox, _session)) = up {
                    outbox
                        .send(bytes)
                        .map_err(|_| eyre::eyre!("the song's relay session to its org is gone"))?;
                }
                Ok(())
            })
        }),
    }
}

/// `branch` of the project at `path` as its org's synced doc, if the
/// project has been shared and this device is signed in; `None` keeps it
/// a file on this device alone.
pub async fn open(path: &Path, store: &Store, branch: &str) -> Option<SyncedBranch> {
    use tracing::Instrument as _;
    let org = org_of(store)?;
    // Its steps as spans under the song's load: the org's sync driver
    // (reaching Task), then the project's docs registered with it.
    let synced = driver(&org)
        .instrument(tracing::info_span!("project_sync.driver"))
        .await?;
    // The branch opened and those it descends from now; the others (old
    // versions, a checkpoint's) behind the open: each is a whole doc to
    // read, seconds apiece on a long-edited song, and none is needed to
    // edit this one.
    let branches = store
        .branches()
        .inspect_err(
            |e| tracing::warn!(sync.org = %org, why = %e, "the project's branches would not read"),
        )
        .ok()?;
    let chain = lineage_of(&branches, branch);
    let mut made = register_branches(
        &synced,
        path,
        store,
        branches,
        |b| chain.iter().any(|c| c == b),
        &HashMap::new(),
    )
    .instrument(tracing::info_span!(
        "project_sync.register",
        sync.branches = chain.len()
    ))
    .await
    .inspect_err(|e| tracing::warn!(sync.org = %org, why = %e, "the project's docs would not open"))
    .ok()?;
    register_rest_later(&synced, path, &made);
    let (id, doc) = made.remove(branch)?;
    tracing::info!(sync.org = %org, sync.doc = %id, "the song is its synced doc");
    Some(SyncedBranch {
        id,
        org,
        replica: synced.files.replica(id),
        doc,
    })
}

/// Start every branch of the project in `store` over from what it holds
/// now: its model read and written into a fresh doc, checked to read back
/// the same, and only then — every branch at once — made the branch's base
/// ([`Store::rebase`]). Returns each branch's state size before and after,
/// in bytes.
///
/// A branch keeps its doc id: this runs once, before the project's first
/// sync ([`share`]), so no copy of the old lineage is anywhere to refuse
/// the new one — and a catalog row a share that never got through left
/// behind (an empty doc) is taken over instead of orphaned.
///
/// The steps the file listed are folded away with the history; keep a copy
/// of the file first if they matter.
///
/// # Errors
///
/// A branch would not load or write, or did not read back the same (then
/// nothing is changed).
pub fn fold_fresh(store: &Store) -> Result<Vec<(String, usize, usize)>, String> {
    use crate::sync::{ORIGIN_LOCAL, SessionDoc};
    let state = |doc: &loro::LoroDoc| {
        doc.export(loro::ExportMode::StateOnly(None))
            .map_or(0, |b| b.len())
    };
    let mut fresh = Vec::new();
    for branch in store.branches().map_err(|e| e.to_string())? {
        let old = store
            .load(&branch.name)
            .map_err(|e| format!("{}: {e}", branch.name))?;
        let before = state(&old);
        let model = SessionDoc::from_loro(old).read();
        let doc = SessionDoc::new();
        doc.write(&model, ORIGIN_LOCAL)
            .map_err(|e| format!("{}: {e}", branch.name))?;
        if doc.read() != model {
            return Err(format!(
                "{}: written fresh, it did not read back the same — nothing changed",
                branch.name
            ));
        }
        let after = state(doc.loro());
        fresh.push((branch.name, doc, before, after));
    }
    for (branch, doc, _, _) in &fresh {
        store
            .rebase(branch, doc.loro())
            .map_err(|e| format!("{branch}: {e}"))?;
    }
    store.set_meta(FRESH_META, "1").map_err(|e| e.to_string())?;
    Ok(fresh
        .into_iter()
        .map(|(branch, _, before, after)| (branch, before, after))
        .collect())
}

/// `<Song>.before-sync.session` beside `path`: the project as it was,
/// history and steps, before [`fold_fresh`].
fn keep_history(path: &Path) -> Result<PathBuf, String> {
    let stem = path
        .file_stem()
        .map_or_else(|| "Song".into(), |s| s.to_string_lossy().into_owned());
    let kept = path.with_file_name(format!("{stem}.before-sync.session"));
    if !kept.exists() {
        std::fs::copy(path, &kept).map_err(|e| format!("keeping {}: {e}", kept.display()))?;
    }
    Ok(kept)
}

/// Share the project at `path` with `org`: from now on it syncs there,
/// from any device it is opened on. Waits until every branch has reached
/// Task, and returns each branch's doc id.
///
/// # Errors
///
/// Not signed in, the file would not open, Task refused a branch, or it
/// did not catch up within [`SHARE_WAIT`].
pub async fn share(path: &Path, org: &str) -> Result<Vec<(String, Uuid)>, String> {
    let mut store = Store::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if store.meta(FRESH_META).ok().flatten().is_none() {
        // One file again before copying it: what the log beside it holds
        // is in the copy.
        store.checkpoint().map_err(|e| e.to_string())?;
        drop(store);
        let kept = keep_history(path)?;
        store = Store::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        for (branch, before, after) in fold_fresh(&store)? {
            tracing::info!(
                song.branch = %branch,
                song.state_before = before,
                song.state_after = after,
                song.history = %kept.display(),
                "the branch starts fresh to sync"
            );
        }
    }
    store.set_meta(ORG_META, org).map_err(|e| e.to_string())?;
    let synced = driver(org)
        .await
        .ok_or_else(|| format!("not signed in, or Task does not know the org `{org}`"))?;
    let mut events = synced.driver.sync_events();
    let made = register(&synced, path, &store).await?;
    let mut waiting: HashMap<Uuid, String> = made
        .iter()
        .map(|(branch, (id, _))| (*id, branch.clone()))
        .collect();
    let mut ids: Vec<(String, Uuid)> = made.iter().map(|(b, (id, _))| (b.clone(), *id)).collect();
    ids.sort();
    let caught_up = architect::platform::timeout(SHARE_WAIT, async {
        use tokio::sync::broadcast::error::RecvError;
        while !waiting.is_empty() {
            match events.recv().await {
                Ok(DocSyncEvent::Attached { doc_id, .. }) => {
                    waiting.remove(&doc_id);
                }
                Ok(DocSyncEvent::Refused { doc_id, reason, .. })
                    if waiting.contains_key(&doc_id) =>
                {
                    return Err(format!(
                        "Task refused `{}`: {reason:?}",
                        waiting.get(&doc_id).map_or("a branch", String::as_str)
                    ));
                }
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return Err("the driver stopped".to_owned()),
            }
        }
        Ok(())
    })
    .await;
    match caught_up {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            return Err(format!(
                "not every branch reached Task within {} s",
                SHARE_WAIT.as_secs()
            ));
        }
    }
    // Caught up means sent, not kept: what the server lacked has left this
    // device. Read every branch back from Task on a connection of its own
    // before saying so — and keep this one open meanwhile, so the server
    // finishes taking what is still on its way (it stops a session's work
    // when the session drops).
    let account = fts_account::signed_in().ok_or("signed out while sharing")?;
    let url = fts_account::sync::org_url(&account.server, org);
    let want: Vec<(Uuid, loro::VersionVector)> = made
        .values()
        .map(|(id, doc)| (*id, doc.loro().oplog_vv()))
        .collect();
    let held =
        architect::platform::timeout(SHARE_WAIT, on_task(&url, &account.session.token, &want))
            .await;
    match held {
        Ok(Ok(())) => Ok(ids),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(format!(
            "sent, but Task did not hold every branch within {} s",
            SHARE_WAIT.as_secs()
        )),
    }
}

/// Wait until Task holds every doc in `want` at least at its version,
/// read through a device of its own (no index, nothing kept) on `url`.
async fn on_task(
    url: &str,
    token: &str,
    want: &[(Uuid, loro::VersionVector)],
) -> Result<(), String> {
    use crdt::docs::{DocsDriver, InMemoryDocsIndex, SharedStorage};
    let reader = DocsDriver::new(
        InMemoryDocsIndex::new(),
        SharedStorage(Arc::new(crdt::in_memory::InMemoryPersistence::new())),
    );
    let lane = fts_account::sync::lane(url, token).await?;
    let running = reader.clone();
    let session = tokio::spawn(async move {
        let _ = running.run(&lane).await;
    });
    let mut left: Vec<(Uuid, loro::VersionVector)> = want.to_vec();
    while !left.is_empty() {
        let mut still = Vec::new();
        for (id, vv) in left {
            let there = match reader.open(id).await {
                Ok(doc) => doc.loro().oplog_vv().includes_vv(&vv),
                Err(_) => false,
            };
            if !there {
                still.push((id, vv));
            }
        }
        left = still;
        if !left.is_empty() {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
    session.abort();
    Ok(())
}

/// Apply what other devices send to an open song from now on: each import
/// into `synced`'s doc wakes `apply`, which runs the bridge's remote pass.
/// Imports that arrive while one runs are taken together. Lives as long as
/// the returned subscription: dropping it (the song closing) ends the task.
pub fn watch_remote<F, Fut>(synced: &SyncedBranch, apply: F) -> crate::sync::loro::Subscription
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    watch_imports(synced.doc.loro(), apply)
}

/// [`watch_remote`] on any doc: each import into `doc` (another device's
/// edit, a live set's relayed in) wakes `apply`; imports that arrive while
/// one runs are taken together. Lives as long as the returned
/// subscription.
pub fn watch_imports<F, Fut>(doc: &loro::LoroDoc, apply: F) -> crate::sync::loro::Subscription
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    let subscription = doc.subscribe_root(Arc::new(move |event| {
        if matches!(
            event.triggered_by,
            crate::sync::loro::EventTriggerKind::Import
        ) {
            let _ = tx.send(());
        }
    }));
    architect::platform::spawn(async move {
        while rx.recv().await.is_some() {
            while rx.try_recv().is_ok() {}
            apply().await;
        }
    });
    subscription
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(name: &str, parent: Option<&str>) -> session_format::store::Branch {
        session_format::store::Branch {
            name: name.into(),
            parent: parent.map(Into::into),
            at: 0,
        }
    }

    #[test]
    fn a_branch_comes_after_the_branch_it_was_forked_from() {
        let ordered = parents_first(vec![
            branch("Drum Edits", Some("Organized")),
            branch("Organized", Some(MAIN)),
            branch(MAIN, None),
            branch("Orphan", Some("gone")),
        ]);
        let names: Vec<&str> = ordered.iter().map(|(n, _)| n.as_str()).collect();
        let at = |n: &str| names.iter().position(|x| *x == n).expect(n);
        assert!(at(MAIN) < at("Organized"));
        assert!(at("Organized") < at("Drum Edits"));
        assert!(names.contains(&"Orphan"), "a lost parent loses no branch");
    }

    #[test]
    fn an_alternative_is_named_after_its_song() {
        let path = Path::new("/songs/Unbreakable.session");
        assert_eq!(doc_name(path, MAIN), "Unbreakable");
        assert_eq!(doc_name(path, "Drum Edits"), "Unbreakable — Drum Edits");
    }
}
