//! A song as one project file: `Song.session`, a Loro doc in a Turso
//! file (`dawfile_store`).
//!
//! The song open in the engine is kept in step with its doc by a bridge
//! (`session::sync::Bridge`) that lives as long as the song is open: it
//! knows which engine FX and item each doc entry is, so a save records
//! only what changed. A save appends that as one update to the song's
//! branch; an open rebuilds the song in the engine from the branch's doc.
//! An alternative or checkpoint is another branch of the same file,
//! forked from the song.
//!
//! Every edit step is history too: once a song is open from (or saved
//! to) its file, each undo step the engine finishes is read into the doc
//! as one commit under the step's name and appended to the file, behind
//! the window ([`record_steps`]). A save is then what folds the file's
//! log back into it.
//!
//! A `Song.session` folder (the format before this) is kept beside the
//! file as `Song.session.legacy` on the first save.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use daw::standalone::Standalone;
use session::sync::engine::{Blobs, Direct, MediaRoot};
use session::sync::{Bridge, SessionDoc};
use session_format::store::{MAIN, Store};

/// A project file's blobs, as the bridge keeps FX state — with the names
/// already stored remembered, since every read of the engine offers each
/// FX's state again and storing one is a compression and a write.
struct StoreBlobs {
    store: Arc<Mutex<Store>>,
    stored: Mutex<std::collections::HashSet<String>>,
}

impl StoreBlobs {
    fn new(store: Arc<Mutex<Store>>) -> Arc<Self> {
        Arc::new(Self {
            store,
            stored: Mutex::default(),
        })
    }
}

impl Blobs for StoreBlobs {
    fn put(&self, bytes: &[u8]) -> String {
        let name = session_format::store::hash_of(bytes);
        let mut stored = self
            .stored
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !stored.contains(&name)
            && with_store(&self.store, |store| {
                store.put_blob("fx", bytes).map_err(|e| e.to_string())
            })
            .is_ok()
        {
            stored.insert(name.clone());
        }
        drop(stored);
        name
    }
    fn get(&self, name: &str) -> Option<Vec<u8>> {
        with_store(&self.store, |store| {
            store.blob(name).map_err(|e| e.to_string())
        })
        .ok()
    }
}

/// The project file at `path`, shared, and `branch`'s doc from it.
/// And how many updates the branch holds since its last compaction.
fn open_branch(
    path: &Path,
    branch: &str,
) -> Result<(Arc<Mutex<Store>>, session::sync::loro::LoroDoc, usize), String> {
    let store = Arc::new(Mutex::new(
        Store::open(path).map_err(|e| format!("{}: {e}", path.display()))?,
    ));
    let doc = with_store(&store, |s| s.load(branch).map_err(|e| e.to_string()))?;
    let uncompacted = with_store(&store, |s| {
        s.steps(branch)
            .map(|steps| steps.len())
            .map_err(|e| e.to_string())
    })
    .unwrap_or(0);
    Ok((store, doc, uncompacted))
}

/// `work` on the store, its lock held for that call only.
fn with_store<T>(
    store: &Mutex<Store>,
    work: impl FnOnce(&Store) -> Result<T, String>,
) -> Result<T, String> {
    let held = store.lock().map_err(|_| "the store's lock")?;
    work(&held)
}

/// A song open from (or saved to) its project file.
struct Open {
    path: PathBuf,
    branch: String,
    store: Arc<Mutex<Store>>,
    bridge: Bridge,
    /// The doc's version at the last save: what the next one appends
    /// from.
    saved: session::sync::loro::VersionVector,
    /// Updates on the branch since its last compaction: past
    /// [`COMPACT_AFTER`], the next save folds them into a snapshot.
    uncompacted: usize,
    /// The branch as its org's synced doc, when the project is shared and
    /// this device signed in (`project_sync`): the bridge edits that doc,
    /// whose own writer appends each step, so [`append`] does not.
    synced: Option<session::project_sync::SyncedBranch>,
    /// Applies what other devices send, while the song is open.
    _remote: Option<session::sync::loro::Subscription>,
}

impl Open {
    /// Name the commit about to be made, for a synced song's history.
    fn name_next(&self, label: &str) {
        if let Some(synced) = &self.synced {
            synced.name_next(label);
        }
    }
}

/// The doc a song's bridge edits: the branch's synced doc when the project
/// is shared (and this device signed in), else the branch as the file
/// loaded it.
async fn song_doc(
    path: &Path,
    store: &Arc<Mutex<Store>>,
    branch: &str,
    loaded: session::sync::loro::LoroDoc,
) -> (
    session::sync::loro::LoroDoc,
    Option<session::project_sync::SyncedBranch>,
) {
    let shared = match store.lock() {
        Ok(held) => session::project_sync::org_of(&held).is_some(),
        Err(_) => false,
    };
    if !shared {
        return (loaded, None);
    }
    // A second handle on the file for the replica: the store's own is held
    // by this song for its blobs and folds.
    let Ok(own) = Store::open(path) else {
        return (loaded, None);
    };
    match session::project_sync::open(path, &own, branch).await {
        Some(synced) => (synced.loro(), Some(synced)),
        None => (loaded, None),
    }
}

/// From now on, what other devices send to the song `guid` reaches its
/// engine through its bridge.
fn watch_remote(
    guid: &str,
    synced: Option<&session::project_sync::SyncedBranch>,
) -> Option<session::sync::loro::Subscription> {
    let guid = guid.to_owned();
    Some(session::project_sync::watch_remote(synced?, move || {
        let guid = guid.clone();
        async move {
            let mut songs = open_songs().lock().await;
            // Not in the open songs yet (still opening): the bridge's watch
            // keeps what the edit touched, and the next pass applies it.
            if let Some(open) = songs.get_mut(&guid)
                && let Err(e) = open.bridge.remote_changed().await
            {
                tracing::warn!(project.guid = %guid, why = %e, "another device's edit did not reach the engine");
            }
        }
    }))
}

/// How many updates a branch keeps before a save folds them into one
/// snapshot ([`Store::compact`]) whatever else happens. An open replays
/// every update since the last snapshot, working the song out again from
/// each: Unbreakable's 377 took 16 s to open, its snapshot 0.2 s.
const COMPACT_AFTER: usize = 64;

/// How long the song is left alone before what it has done since its
/// snapshot is folded into a new one ([`compact_when_idle`]). Even a few
/// updates on a snapshot make an open read the whole history (36 on
/// Unbreakable's: 7.9 s, against 0.2 s for the snapshot alone).
const IDLE: std::time::Duration = std::time::Duration::from_secs(10);

/// When a song's file was last written to.
static LAST_WRITE: Mutex<Option<web_time::Instant>> = Mutex::new(None);

/// Fold every open song's updates into a snapshot once the songs have been
/// left alone for [`IDLE`] — in the background, under each song's lock (so
/// no step is appended while it folds), shown in the bottom bar while it
/// works. Started once, with the step recorder.
fn compact_when_idle() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let spawned = std::thread::Builder::new()
            .name("song-compact".into())
            .spawn(|| {
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    let quiet = LAST_WRITE
                        .lock()
                        .ok()
                        .and_then(|at| *at)
                        .is_some_and(|at| at.elapsed() >= IDLE);
                    if !quiet {
                        continue;
                    }
                    if let Ok(mut at) = LAST_WRITE.lock() {
                        *at = None;
                    }
                    let folded = run(async {
                        let mut songs = open_songs().lock().await;
                        for open in songs.values_mut().filter(|o| o.uncompacted > 0) {
                            let _job = crate::jobs::start("Saving the song's history", 0);
                            let mut stages = crate::stages::Stages::start();
                            let doc = open.bridge.doc();
                            if let Err(e) = with_store(&open.store, |s| {
                                s.compact(&open.branch, doc.loro()).map_err(|e| e.to_string())
                            }) {
                                tracing::warn!(song.error = %e, "the song's history could not be folded");
                                continue;
                            }
                            stages.mark("song.compact_ms");
                            open.uncompacted = 0;
                        }
                    });
                    if let Err(e) = folded {
                        tracing::warn!(song.error = %e, "folding the songs' history did not run");
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::warn!(song.error = %e, "no thread to fold the songs' history on");
        }
    });
}

/// The open songs, by the engine's project guid.
fn open_songs() -> &'static tokio::sync::Mutex<HashMap<String, Open>> {
    static OPEN: std::sync::LazyLock<tokio::sync::Mutex<HashMap<String, Open>>> =
        std::sync::LazyLock::new(Default::default);
    &OPEN
}

/// Whether `path` is a project file — a `Song.session` that is a file,
/// not the folder of the format before.
#[must_use]
pub fn is_project_file(path: &Path) -> bool {
    crate::open_core::is_session(path) && path.is_file()
}

/// The project file and branch a song path names: `Song.session` (a
/// file) is the song, `Song.session/Organized` its alternative; `None`
/// for anything else (an `.RPP`, a `.session` folder).
#[must_use]
pub fn file_and_branch(path: &Path) -> Option<(PathBuf, String)> {
    if is_project_file(path) {
        return Some((path.to_path_buf(), MAIN.to_owned()));
    }
    let file = path.parent()?;
    let branch = path.file_name()?.to_string_lossy().into_owned();
    is_project_file(file).then(|| (file.to_path_buf(), branch))
}

/// Where a song saves: its project file and branch — an alternative's own
/// branch, else the song's file beside whatever it was opened from.
#[must_use]
pub fn save_target(opened_from: &Path) -> (PathBuf, String) {
    file_and_branch(opened_from).unwrap_or_else(|| {
        let file = if crate::open_core::is_session(opened_from) {
            opened_from.to_path_buf()
        } else {
            opened_from.with_extension("session")
        };
        (file, MAIN.to_owned())
    })
}

/// The project file's own runtime: a song opens before the app's engine
/// runtime and facade are up (at launch), so its work never waits on them.
fn runtime() -> Result<&'static tokio::runtime::Runtime, String> {
    static RUNTIME: std::sync::LazyLock<Option<tokio::runtime::Runtime>> =
        std::sync::LazyLock::new(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("project-file")
                .thread_stack_size(16 * 1024 * 1024)
                .enable_all()
                .build()
                .ok()
        });
    RUNTIME
        .as_ref()
        .ok_or_else(|| "no runtime for the project file".to_owned())
}

/// Run `work` to completion on the project file's runtime, from any
/// thread — an action's, the window's, or a runtime's own.
fn run<T: Send>(work: impl std::future::Future<Output = T> + Send) -> Result<T, String> {
    let runtime = runtime()?;
    std::thread::scope(|scope| {
        scope
            .spawn(|| runtime.block_on(work))
            .join()
            .map_err(|_| "the project file's work panicked".to_owned())
    })
}

/// A facade on `daw` of the project file's own — served in process, on
/// its runtime — kept while it is the same engine.
async fn client(daw: &Standalone) -> Result<daw_control::Daw, String> {
    // The whole bundle is kept: it owns the task serving the link.
    static CLIENT: std::sync::LazyLock<
        tokio::sync::Mutex<Option<daw::standalone::bootstrap::InProcessDaw>>,
    > = std::sync::LazyLock::new(Default::default);
    let mut held = CLIENT.lock().await;
    let kept = held
        .as_ref()
        .filter(|bundle| bundle.standalone.same_engine(daw))
        .map(|bundle| bundle.daw.clone());
    if let Some(client) = kept {
        drop(held);
        return Ok(client);
    }
    let built = daw::standalone::bootstrap::build_in_process_daw(daw.clone())
        .await
        .map_err(|e| e.to_string())?;
    let client = built.daw.clone();
    *held = Some(built);
    drop(held);
    Ok(client)
}

/// `daw`'s project `guid`, through the project file's facade.
async fn project(daw: &Standalone, guid: &str) -> Result<daw_control::Project, String> {
    client(daw)
        .await?
        .project(guid.to_owned())
        .await
        .map_err(|e| e.to_string())
}

fn direct(daw: &Standalone) -> Arc<dyn Direct> {
    Arc::new(daw.clone())
}

fn media_of(path: &Path) -> MediaRoot {
    MediaRoot(path.parent().map(Path::to_path_buf).unwrap_or_default())
}

/// Save the song (the engine's `project_guid`) into the project file at
/// `path` on `branch`. Says where.
///
/// What changed since its last save goes in as one update named `label`.
/// A `Song.session` folder there is kept beside it as
/// `Song.session.legacy`.
///
/// # Errors
///
/// The file would not open or take the write, or the engine would not be
/// read.
pub fn save(
    daw: &Standalone,
    project_guid: &str,
    path: &Path,
    branch: &str,
    label: &str,
) -> Result<PathBuf, String> {
    let _job = crate::jobs::start("Saving", 0);
    if path.is_dir() {
        let legacy = PathBuf::from(format!("{}.legacy", path.display()));
        std::fs::rename(path, &legacy)
            .map_err(|e| format!("keeping {} as {}: {e}", path.display(), legacy.display()))?;
    }
    let daw = daw.clone();
    let guid = project_guid.to_owned();
    let path = path.to_path_buf();
    let branch = branch.to_owned();
    let label = label.to_owned();
    run(async move {
        let mut songs = open_songs().lock().await;
        let same = songs
            .get(&guid)
            .is_some_and(|open| open.path == path && open.branch == branch);
        if !same {
            // Into this file for the first time (or another file): its
            // history so far, the song written over it.
            // A copy saved elsewhere leaves the song's own bridge — and
            // what it knows of the song's ids — where it is.
            if songs.contains_key(&guid) {
                drop(songs);
                let mut copy = begin(&daw, &guid, &path, &branch).await?;
                return save_open(&mut copy, &label).await.map(|()| path);
            }
            let mut open = begin(&daw, &guid, &path, &branch).await?;
            open._remote = watch_remote(&guid, open.synced.as_ref());
            songs.insert(guid.clone(), open);
        }
        let saved = match songs.get_mut(&guid) {
            Some(open) => save_open(open, &label).await,
            None => Err("the song is not open".to_owned()),
        };
        drop(songs);
        saved.map(|()| path)
    })?
}

/// The song `guid` into the project file at `path` on `branch`: the
/// branch's history so far, a bridge reading the song over it.
async fn begin(daw: &Standalone, guid: &str, path: &Path, branch: &str) -> Result<Open, String> {
    record_steps();
    let (store, loaded, uncompacted) = open_branch(path, branch)?;
    let (loaded, synced) = song_doc(path, &store, branch, loaded).await;
    let doc = SessionDoc::from_loro(loaded);
    let saved = doc.version();
    let chart = doc.read().chart;
    if let Some(synced) = &synced {
        synced.name_next("Save");
    }
    let bridge = Bridge::host(
        project(daw, guid).await?,
        media_of(path),
        doc,
        chart,
        StoreBlobs::new(store.clone()),
        Some(direct(daw)),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(Open {
        path: path.to_path_buf(),
        branch: branch.to_owned(),
        store,
        bridge,
        saved,
        uncompacted,
        synced,
        _remote: None,
    })
}

/// The song as it is now into its file: read, and what changed appended.
async fn save_open(open: &mut Open, label: &str) -> Result<(), String> {
    open.name_next(label);
    open.bridge
        .local_changed()
        .await
        .map_err(|e| e.to_string())?;
    append(open, label)?;
    // One file again: what was written folded out of the log beside it.
    with_store(&open.store, |s| s.checkpoint().map_err(|e| e.to_string()))
}

/// Append what the doc holds past its last save to the song's branch.
fn append(open: &mut Open, label: &str) -> Result<(), String> {
    let doc = open.bridge.doc();
    if doc.version() != open.saved {
        // A synced song's doc appends its own commits (named by
        // `Open::name_next`); appending here too would write each twice.
        if open.synced.is_none() {
            let update = doc.updates_since(&open.saved).map_err(|e| e.to_string())?;
            with_store(&open.store, |s| {
                s.append_update(&open.branch, label, &update)
                    .map_err(|e| e.to_string())
            })?;
        }
        open.saved = doc.version();
        open.uncompacted = open.uncompacted.saturating_add(1);
        if let Ok(mut at) = LAST_WRITE.lock() {
            *at = Some(web_time::Instant::now());
        }
    }
    // Folded into a snapshot once there are enough to slow an open. Safe
    // here: the doc holds every update appended (it is at or past
    // `saved`), and nothing appends while this `Open` is held — whatever
    // the snapshot holds beyond them is appended again and merged as the
    // same operations.
    if open.uncompacted >= COMPACT_AFTER {
        let mut stages = crate::stages::Stages::start();
        with_store(&open.store, |s| {
            s.compact(&open.branch, doc.loro())
                .map_err(|e| e.to_string())
        })?;
        stages.mark("song.compact_ms");
        open.uncompacted = 0;
    }
    Ok(())
}

/// Record each edit step the engine finishes into its song's file, once
/// that song is open from it — on a thread of its own, so the window
/// never waits on a read of the engine. Steps that finish while one is
/// being recorded are read together, as one commit named for them all.
/// Steps the engine finished that the recorder has not recorded yet.
static PENDING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// How the kit's MIDI agrees with its hits after `labels` (the steps
/// recorded together): an undo or a redo puts both back, so the views are
/// written from the hits; any other step may be an edit of a view.
fn sync_mode(labels: &[String]) -> crate::hits_midi::Mode {
    if labels
        .iter()
        .any(|l| l.starts_with("Undo ") || l.starts_with("Redo "))
    {
        crate::hits_midi::Mode::Follow
    } else {
        crate::hits_midi::Mode::Reconcile
    }
}

pub fn record_steps() {
    compact_when_idle();
    static LISTENING: std::sync::Once = std::sync::Once::new();
    LISTENING.call_once(|| {
        // Each step with the span it finished in (the action that made
        // it): its recording is traced under that.
        let (tx, rx) = std::sync::mpsc::channel::<(String, String, tracing::Span)>();
        let tx = Mutex::new(tx);
        daw::standalone::steps::listen(Arc::new(move |project, label| {
            if let Ok(tx) = tx.lock()
                && tx
                    .send((project.to_owned(), label.to_owned(), tracing::Span::current()))
                    .is_ok()
            {
                PENDING.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }));
        let _ = std::thread::Builder::new()
            .name("project-file-steps".into())
            .spawn(move || {
                while let Ok(first) = rx.recv() {
                    let mut steps: Vec<(String, Vec<String>, tracing::Span)> = Vec::new();
                    let mut taken = 0_usize;
                    for (project, label, made_in) in std::iter::once(first).chain(rx.try_iter()) {
                        taken = taken.saturating_add(1);
                        match steps.iter_mut().find(|(p, _, _)| *p == project) {
                            Some((_, labels, _)) => labels.push(label),
                            None => steps.push((project, vec![label], made_in)),
                        }
                    }
                    for (project, labels, made_in) in steps {
                        // The steps' recording, under the action that made
                        // the first of them (it may outlast it).
                        let step = tracing::info_span!(
                            parent: &made_in,
                            "song.step",
                            steps.labels = labels.len(),
                            steps.label = labels.first().map(String::as_str).unwrap_or_default(),
                            steps.views_edited = tracing::field::Empty,
                            steps.hits_added = tracing::field::Empty,
                            steps.hits_moved = tracing::field::Empty,
                            steps.hits_removed = tracing::field::Empty,
                        );
                        let _step = step.enter();
                        let mut stages = crate::stages::Stages::start();
                        // What is made from the recording follows it first —
                        // the kit's samples onto where its hits play now — so
                        // it is in the same commit as the edit that moved it.
                        if let Some(daw) = crate::open::local_engine() {
                            crate::actions::samples_follow(
                                daw,
                                &daw::service::ProjectContext::Project(project.clone()),
                                sync_mode(&labels),
                            );
                        }
                        stages.mark("step.follow_ms");
                        if let Err(why) = record(&project, &labels.join("; ")) {
                            tracing::warn!(project.guid = %project, why, "an edit step was not recorded");
                        }
                        stages.mark("step.record_ms");
                    }
                    PENDING.fetch_sub(taken, std::sync::atomic::Ordering::SeqCst);
                }
            });
    });
}

/// The engine finished the step `label` in the song `project_guid`: read
/// it into the song's doc as one commit and append that to its file — if
/// the song is open from one.
fn record(project_guid: &str, label: &str) -> Result<(), String> {
    let guid = project_guid.to_owned();
    let label = label.to_owned();
    // On the project file's runtime, still under the step's span.
    run(tracing::Instrument::in_current_span(async move {
        let mut songs = open_songs().lock().await;
        let Some(open) = songs.get_mut(&guid) else {
            return Ok(());
        };
        open.name_next(&label);
        let recorded = match tracing::Instrument::instrument(
            open.bridge.local_step(&label),
            tracing::info_span!("bridge.local_step"),
        )
        .await
        {
            Ok(true) => {
                tracing::info_span!("project_file.append").in_scope(|| append(open, &label))
            }
            Ok(false) => Ok(()),
            Err(e) => Err(e.to_string()),
        };
        drop(songs);
        recorded
    }))?
}

/// What an undo or redo through a song's doc came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Undone {
    /// The song is not open from a project file: its engine's own undo
    /// is the one there is.
    NotOpen,
    /// Nothing of this peer's to undo (or redo).
    Nothing,
    /// The step of this name was undone (or redone).
    Step(String),
}

/// Undo the last step of the song `project_guid` through its doc —
/// this peer's own, others' edits since left standing — the engine made
/// to match and the undo appended to its file. A step the engine finished
/// that is not recorded yet is recorded first, so it is the one undone.
///
/// # Errors
/// The engine would not be read or made to match, or the file refused
/// the write.
pub fn undo(daw: &Standalone, project_guid: &str) -> Result<Undone, String> {
    through_doc(daw, project_guid, true)
}

/// Redo the step last undone in the song `project_guid`, as [`undo`].
///
/// # Errors
/// As [`undo`].
pub fn redo(daw: &Standalone, project_guid: &str) -> Result<Undone, String> {
    through_doc(daw, project_guid, false)
}

fn through_doc(daw: &Standalone, project_guid: &str, back: bool) -> Result<Undone, String> {
    let pending = daw::service::Projects::last_undo_label(
        daw,
        daw_proto::ProjectContext::Project(project_guid.to_owned()),
    )
    .unwrap_or_else(|| "Edits".to_owned());
    let guid = project_guid.to_owned();
    run(async move {
        let mut songs = open_songs().lock().await;
        let Some(open) = songs.get_mut(&guid) else {
            return Ok(Undone::NotOpen);
        };
        let done = step_back(open, &pending, back).await;
        drop(songs);
        done
    })?
}

async fn step_back(open: &mut Open, pending: &str, back: bool) -> Result<Undone, String> {
    // A step the recorder has not reached yet is recorded first, so it is
    // the one undone; with none waiting, nothing to read.
    if PENDING.load(std::sync::atomic::Ordering::SeqCst) > 0 {
        open.name_next(pending);
        if open
            .bridge
            .local_step(pending)
            .await
            .map_err(|e| e.to_string())?
        {
            append(open, pending)?;
        }
    }
    open.name_next(if back { "Undo" } else { "Redo" });
    let step = if back {
        open.bridge.undo().await
    } else {
        open.bridge.redo().await
    }
    .map_err(|e| e.to_string())?;
    let Some(name) = step else {
        return Ok(Undone::Nothing);
    };
    append(
        open,
        &format!("{} {name}", if back { "Undo" } else { "Redo" }),
    )?;
    Ok(Undone::Step(name))
}

/// Open `branch` of the project file at `path` as a new song in `daw`:
/// its doc rebuilt in the engine. Its media resolves against the file's
/// folder. Returns the engine's project guid.
///
/// The song is built on a stage ([`Standalone::stage`]) — off the engine's
/// lock, on a thread of its own ([`build_on`]) — and landed whole, so a
/// song opening never stalls a reader of another, and songs opening at
/// once build side by side.
///
/// # Errors
///
/// The file would not open or load, or the engine refused to rebuild it.
pub fn load(daw: &Standalone, name: &str, path: &Path, branch: &str) -> Result<String, String> {
    let _job = crate::jobs::start("Opening", 0);
    record_steps();
    // The project file's load, one unit of work under the song's open: the
    // file, its doc, the build and the landing, each timed on it.
    let span = tracing::info_span!(
        "project_file.load",
        load.store_ms = tracing::field::Empty,
        load.doc_ms = tracing::field::Empty,
        load.build_ms = tracing::field::Empty,
        load.land_ms = tracing::field::Empty,
        load.uncompacted = tracing::field::Empty,
    );
    let _load = span.enter();
    let mut stages = crate::stages::Stages::start();
    let (store, loaded, uncompacted) =
        tracing::info_span!("project_file.open_branch").in_scope(|| open_branch(path, branch))?;
    span.record("load.store_ms", stages.mark("load.store_ms"));
    span.record("load.uncompacted", uncompacted);
    // A backlog on the snapshot is folded once the song is left alone.
    if uncompacted > 0
        && let Ok(mut at) = LAST_WRITE.lock()
    {
        *at = Some(web_time::Instant::now());
    }
    // A shared project opens as its synced doc: what this device missed
    // is merged in as the driver catches up.
    let (loaded, synced) = run(tracing::Instrument::instrument(
        song_doc(path, &store, branch, loaded),
        tracing::info_span!("project_file.song_doc"),
    ))?;
    let doc =
        tracing::info_span!("project_file.from_loro").in_scope(|| SessionDoc::from_loro(loaded));
    let saved = doc.version();
    span.record("load.doc_ms", stages.mark("load.doc_ms"));
    let guid = uuid::Uuid::new_v4().to_string();
    let stage = daw.stage();
    stage.engine().seed_project(daw_proto::ProjectInfo {
        guid: guid.clone(),
        name: name.to_owned(),
        path: path.to_string_lossy().into_owned(),
    });
    let built = build_on(
        &stage,
        &guid,
        media_of(path),
        doc,
        StoreBlobs::new(store.clone()),
    )?;
    span.record("load.build_ms", stages.mark("load.build_ms"));
    let daw = daw.clone();
    let path = path.to_path_buf();
    let branch = branch.to_owned();
    let made = guid.clone();
    run(async move {
        // On the project file's runtime: the song's transport lives on it.
        stage.land(&made).map_err(|e| e.to_string())?;
        let bridge = built.moved_to(project(&daw, &made).await?, Some(direct(&daw)));
        let remote = watch_remote(&made, synced.as_ref());
        open_songs().lock().await.insert(
            made,
            Open {
                path,
                branch,
                store,
                bridge,
                saved,
                uncompacted,
                synced,
                _remote: remote,
            },
        );
        Ok::<(), String>(())
    })??;
    span.record("load.land_ms", stages.mark("load.land_ms"));
    Ok(guid)
}

/// The song `guid` rebuilt from `doc` on `stage`, and the bridge that
/// keeps it — on a thread and a runtime of its own, the stage's facade
/// served on that same thread: nothing a build does waits on another's,
/// or on the project file's two workers.
fn build_on(
    stage: &daw::standalone::Stage,
    guid: &str,
    media: MediaRoot,
    doc: SessionDoc,
    blobs: Arc<StoreBlobs>,
) -> Result<Bridge, String> {
    let engine = stage.engine();
    // The build is the load's (`project.load`), though on a thread of its
    // own: its facade calls land under it, the slow ones as spans.
    let build = tracing::info_span!("song.build");
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("song-build".to_owned())
            .stack_size(16 * 1024 * 1024)
            .spawn_scoped(scope, || {
                let _build = build.enter();
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| format!("no runtime to build the song on: {e}"))?;
                runtime.block_on(async {
                    let facade = daw::standalone::bootstrap::build_in_process_daw(engine.clone())
                        .await
                        .map_err(|e| e.to_string())?;
                    let project = facade
                        .daw
                        .project(guid.to_owned())
                        .await
                        .map_err(|e| e.to_string())?;
                    Bridge::join(project, media, doc, blobs, Some(direct(engine)))
                        .await
                        .map_err(|e| e.to_string())
                })
            })
            .map_err(|e| format!("no thread to build the song on: {e}"))?
            .join()
            .map_err(|_| "building the song panicked".to_owned())?
    })
}

/// Fork `name` from the song open as `project_guid`, as it is now.
///
/// `path` is what it was opened from: the song or one of its
/// alternatives. The song is saved first, then the branch made at that
/// point, from its own. The song stays what is open. Says the project
/// file.
///
/// # Errors
///
/// A name with a path separator in it or none at all, or as [`save`].
pub fn fork(
    daw: &Standalone,
    project_guid: &str,
    path: &Path,
    name: &str,
) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\']) || name == session_format::store::MAIN {
        return Err(format!("{name:?} is not a name an alternative can have"));
    }
    let (file, branch) = save_target(path);
    save(daw, project_guid, &file, &branch, &format!("Before {name}"))?;
    let guid = project_guid.to_owned();
    let name = name.to_owned();
    run(async move {
        let songs = open_songs().lock().await;
        let open = songs.get(&guid).ok_or("the song is not open")?;
        let forked = with_store(&open.store, |s| {
            s.fork(&name, &open.branch, open.bridge.doc().loro())
                .and_then(|()| s.checkpoint())
                .map_err(|e| e.to_string())
        });
        drop(songs);
        forked
    })??;
    Ok(file)
}

/// Every branch of the project file at `path`: the song, then its
/// alternatives and checkpoints.
///
/// # Errors
///
/// The file would not open.
pub fn branches(path: &Path) -> Result<Vec<session_format::store::Branch>, String> {
    Store::open(path)
        .and_then(|store| store.branches())
        .map_err(|e| e.to_string())
}

/// The doc `path` (a project file, or an alternative in one) holds, as
/// saved; `None` for anything else.
#[must_use]
pub fn doc_of(path: &Path) -> Option<session::sync::loro::LoroDoc> {
    let (file, branch) = file_and_branch(path)?;
    Store::open(&file).ok()?.load(&branch).ok()
}

/// Whether the song open as `project_guid` is what its project file
/// (`path`) reopens as.
///
/// The file is rebuilt in a second project and both are read as the doc
/// keeps them (its own ids, not the engine's), compared part by part.
/// Empty when the same; else what differs. The second project is closed
/// again.
///
/// # Errors
///
/// The file would not open, or either song would not be read.
pub fn check(daw: &Standalone, project_guid: &str, path: &Path) -> Result<Vec<String>, String> {
    let (file, branch) = save_target(path);
    let reopened = load(daw, "check", &file, &branch)?;
    let live_guid = project_guid.to_owned();
    let daw_for = daw.clone();
    let apart = run(async move {
        let saved = open_songs()
            .lock()
            .await
            .remove(&reopened)
            .ok_or("the saved song did not open")?
            .bridge
            .shown()
            .clone();
        // The live song as the doc reads it: through its own bridge when
        // it has one (its ids), else read afresh.
        let own = live_read(&live_guid).await?;
        let live = match own {
            Some(live) => live,
            None => Bridge::host(
                project(&daw_for, &live_guid).await?,
                media_of(&file),
                SessionDoc::new(),
                saved.chart.clone(),
                Arc::new(session::sync::engine::MemoryBlobs::default()),
                Some(direct(&daw_for)),
            )
            .await
            .map_err(|e| e.to_string())?
            .shown()
            .clone(),
        };
        Ok::<_, String>((reopened, differences(&live, &saved)))
    })??;
    let _ = daw::service::Projects::close(daw, &apart.0);
    Ok(apart.1)
}

/// The song `guid` as its own bridge reads it now, if it has one.
async fn live_read(guid: &str) -> Result<Option<session::sync::SessionModel>, String> {
    let mut songs = open_songs().lock().await;
    let Some(open) = songs.get_mut(guid) else {
        return Ok(None);
    };
    let read = open.bridge.local_changed().await.map_err(|e| e.to_string());
    let shown = open.bridge.shown().clone();
    drop(songs);
    read.map(|_| Some(shown))
}

/// What differs between two songs as the doc reads them, part by part:
/// each track and item that is not the same, by its guid, first few.
fn differences(
    live: &session::sync::SessionModel,
    saved: &session::sync::SessionModel,
) -> Vec<String> {
    const SHOWN: usize = 4;
    let mut out = Vec::new();
    let tracks: Vec<String> = live
        .tracks
        .iter()
        .filter(|t| saved.track(&t.guid) != Some(*t))
        .map(|t| {
            let parts = saved.track(&t.guid).map_or_else(String::new, |s| {
                [
                    ("fx", t.fx != s.fx),
                    ("sends", t.sends != s.sends),
                    ("envelopes", t.envelopes != s.envelopes),
                    ("lanes", t.lanes != s.lanes),
                    ("groups", t.grouping != s.grouping),
                ]
                .into_iter()
                .filter(|(_, differs)| *differs)
                .map(|(part, _)| part)
                .collect::<Vec<_>>()
                .join(", ")
            });
            // What each side's chain is, when that is what differs.
            let chains =
                saved
                    .track(&t.guid)
                    .filter(|s| s.fx != t.fx)
                    .map_or_else(String::new, |s| {
                        let chain = |fx: &[session::sync::FxState]| {
                            fx.iter()
                                .map(|f| {
                                    format!(
                                        "{}{}",
                                        f.name,
                                        f.state.as_deref().map_or_else(String::new, |h| {
                                            format!("@{}", h.get(..8).unwrap_or(h))
                                        })
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join(" ")
                        };
                        format!(" (live [{}], saved [{}])", chain(&t.fx), chain(&s.fx))
                    });
            let lanes = saved
                .track(&t.guid)
                .filter(|s| s.lanes != t.lanes)
                .map_or_else(String::new, |s| {
                    let lane = |l: &session::sync::LaneState| {
                        format!(
                            "{} lanes, playing {:b}, named {:?}",
                            l.count, l.play_mask, l.names
                        )
                    };
                    format!(" (live {}; saved {})", lane(&t.lanes), lane(&s.lanes))
                });
            format!("track {} {:?}: {parts}{chains}{lanes}", t.guid, t.name)
        })
        .chain(
            saved
                .tracks
                .iter()
                .filter(|t| live.track(&t.guid).is_none())
                .map(|t| format!("track {} {:?} (only saved)", t.guid, t.name)),
        )
        .collect();
    if !tracks.is_empty() {
        out.push(format!("tracks: {} differ", tracks.len()));
        out.extend(tracks.into_iter().take(SHOWN).map(|l| format!("  {l}")));
    }
    let items: Vec<String> = live
        .items
        .iter()
        .filter(|(g, i)| saved.items.get(*g) != Some(*i))
        .map(|(g, _)| format!("item {g}"))
        .chain(
            saved
                .items
                .keys()
                .filter(|g| !live.items.contains_key(*g))
                .map(|g| format!("item {g} (only saved)")),
        )
        .collect();
    if !items.is_empty() {
        out.push(format!("items: {} differ", items.len()));
        out.extend(items.into_iter().take(SHOWN).map(|l| format!("  {l}")));
    }
    for (what, same) in [
        ("markers", live.markers == saved.markers),
        ("regions", live.regions == saved.regions),
        ("tempo", live.tempo == saved.tempo),
    ] {
        if !same {
            out.push(format!("{what} differ"));
        }
    }
    out
}
