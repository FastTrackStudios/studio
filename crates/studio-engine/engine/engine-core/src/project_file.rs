//! A song as one project file: `Song.session`, a Loro doc in a Turso
//! file (`dawfile_store`).
//!
//! The song open in the engine is kept in step with its doc by a bridge
//! (`engine_sync::Bridge`) that lives as long as the song is open: it
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
use engine_sync::engine::{Blobs, Direct, MediaRoot};
use engine_sync::{Bridge, SessionDoc};
use session_format::store::{MAIN, Moment, MomentRef, Store};

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
) -> Result<(Arc<Mutex<Store>>, engine_sync::loro::LoroDoc, usize), String> {
    let store = Arc::new(Mutex::new(
        Store::open(path).map_err(|e| format!("{}: {e}", path.display()))?,
    ));
    let doc = with_store(&store, |s| s.load(branch).map_err(|e| e.to_string()))?;
    with_store(&store, |s| {
        tidy(s, branch, &doc);
        Ok(())
    })?;
    let uncompacted = with_store(&store, |s| {
        s.steps(branch)
            .map(|steps| steps.len())
            .map_err(|e| e.to_string())
    })
    .unwrap_or(0);
    Ok((store, doc, uncompacted))
}

/// The meta key that says a branch has been [`tidy`]-ed.
fn tidied_key(branch: &str) -> String {
    format!("tidied.{branch}")
}

/// Once per branch, as it loads: what an older build left in a song that
/// it no longer keeps, taken out, so the song's next fold is small.
///
/// - The entity maps of deleted items and take markers, which Loro kept
///   whole (`engine_sync::doc::clear_deleted_entities`).
/// - The mixes' plugin states kept inline in every snapshot, moved onto
///   references, each kept once ([`crate::mix_states::compact`]).
/// - The project's ext, one value, written key by key (by the write).
///
/// Here, before the song's open version is taken, so a fold (shallow at
/// that version) drops what was taken out with the history before it.
/// Written to the file as one step, then marked done. On Unbreakable:
/// 186 090 maps cleared and 280 MB of states made 8 MB; its shallow
/// snapshot 173.5 MB to about 80 MB.
///
/// A new or empty branch is only marked: it has nothing to take out. A
/// tidy that fails says so on its span and the song opens as it is; the
/// next open tries again.
fn tidy(store: &Store, branch: &str, doc: &engine_sync::loro::LoroDoc) {
    let key = tidied_key(branch);
    if matches!(store.meta(&key), Ok(Some(_))) {
        return;
    }
    let span = tracing::info_span!(
        "project_file.tidy",
        tidy.cleared = tracing::field::Empty,
        tidy.states = tracing::field::Empty,
        tidy.bytes = tracing::field::Empty,
        tidy.error = tracing::field::Empty,
    );
    let _in = span.enter();
    let tidied = if doc.oplog_vv().is_empty() {
        Ok(())
    } else {
        tidy_doc(store, branch, doc, &span)
    };
    if let Err(e) = tidied.and_then(|()| store.set_meta(&key, "1").map_err(|e| e.to_string())) {
        span.record("tidy.error", e.as_str());
        tracing::warn!("the song could not be tidied; it opens as it is");
    }
}

/// [`tidy`]'s work on a song with something in it: taken out, and written
/// to the file as one step when anything changed.
fn tidy_doc(
    store: &Store,
    branch: &str,
    doc: &engine_sync::loro::LoroDoc,
    span: &tracing::Span,
) -> Result<(), String> {
    let before = doc.oplog_vv();
    let cleared = engine_sync::doc::clear_deleted_entities(doc).map_err(|e| e.to_string())?;
    let song = SessionDoc::from_loro(doc.clone());
    let mut model = song.read();
    let states = crate::mix_states::compact(&mut model.project.ext);
    song.write(&model, engine_sync::ORIGIN_LOCAL)
        .map_err(|e| e.to_string())?;
    span.record("tidy.cleared", cleared);
    span.record("tidy.states", states);
    if doc.oplog_vv() != before {
        let update = doc
            .export(engine_sync::loro::ExportMode::updates(&before))
            .map_err(|e| e.to_string())?;
        span.record("tidy.bytes", update.len());
        store
            .append_update(branch, "Tidy the song", &update)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
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
    saved: engine_sync::loro::VersionVector,
    /// Updates on the branch since its last compaction: past
    /// [`COMPACT_AFTER`], the next save folds them into a snapshot.
    uncompacted: usize,
    /// Applies what reaches the song's doc from elsewhere (a live set's
    /// doc of it), while the song is open.
    _remote: Option<engine_sync::loro::Subscription>,
    /// The doc's version when the song was opened: where a fold makes it
    /// shallow ([`take_fold`]), so this session's steps stay in the file.
    opened_at: engine_sync::loro::Frontiers,
    /// The song's archive (`Archive/<Title>.history`), opened at its first
    /// fold.
    archive: Option<Arc<Mutex<Store>>>,
}

impl Open {
    /// The song's archive, opened (and its folder made) the first time.
    fn archive(&mut self) -> Result<Arc<Mutex<Store>>, String> {
        if let Some(archive) = &self.archive {
            return Ok(Arc::clone(archive));
        }
        let path = with_store(&self.store, |s| {
            s.archive_path(&self.path).map_err(|e| e.to_string())
        })?;
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        }
        let archive = Arc::new(Mutex::new(
            Store::open(&path).map_err(|e| format!("{}: {e}", path.display()))?,
        ));
        self.archive = Some(Arc::clone(&archive));
        Ok(archive)
    }
}

/// Fold the song `guid`'s history now, as an idle fold would: shallow at the
/// version it was opened at, what came before into its archive. What a
/// close or an explicit save calls, and the tests.
///
/// # Errors
/// The song is not open from its file, or the fold failed.
pub fn fold(guid: &str) -> Result<(), String> {
    written(WRITE_WAIT);
    let guid = guid.to_owned();
    let taken = run(async move {
        let mut songs = open_songs().lock().await;
        let open = songs
            .get_mut(&guid)
            .ok_or("the song is not open from its file")?;
        let snapshot = take_fold(open)?;
        open.uncompacted = 0;
        Ok::<_, String>((Arc::clone(&open.store), snapshot))
    })??;
    write_snapshot_later(&taken.0, taken.1, false);
    Ok(())
}

/// The snapshot to fold `open`'s branch into: shallow at the version the
/// song was opened at, its history before that put in its archive first
/// (`Store::take_shallow`), so the file opens small and fast.
fn take_fold(open: &mut Open) -> Result<session_format::store::Snapshot, String> {
    let archive = open.archive()?;
    let doc = open.bridge.doc();
    with_store(&open.store, |s| {
        with_store(&archive, |a| {
            s.take_shallow(&open.branch, doc.loro(), &open.opened_at, a)
                .map_err(|e| e.to_string())
        })
    })
}

/// From now on, what reaches the song `guid`'s doc from elsewhere — a
/// live set's doc of it ([`live_side`]) — reaches its engine through its
/// bridge, and its file.
fn watch_remote(guid: &str, doc: &engine_sync::loro::LoroDoc) -> engine_sync::loro::Subscription {
    let guid = guid.to_owned();
    watch_imports(doc, move || {
        let guid = guid.clone();
        async move {
            // A step read and still being written: the doc is behind what
            // the bridge shows until it is, and a read back now would undo
            // it in the engine.
            let began = web_time::Instant::now();
            while WRITING.load(std::sync::atomic::Ordering::SeqCst) > 0
                && began.elapsed() < WRITE_WAIT
            {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            let mut songs = open_songs().lock().await;
            // Not in the open songs yet (still opening): the bridge's watch
            // keeps what the edit touched, and the next pass applies it.
            let Some(open) = songs.get_mut(&guid) else {
                return;
            };
            match open.bridge.remote_changed().await {
                Ok(changes) => {
                    let label = remote_label(&open.bridge.doc().messages_since(&open.saved));
                    if let Err(e) = append(open, &label) {
                        tracing::warn!(project.guid = %guid, why = %e, "another device's edit was not written to the song's file");
                    }
                    drop(songs);
                    if !changes.is_empty()
                        && crate::open::current_song().as_deref() == Some(guid.as_str())
                    {
                        crate::song_model::request_resync();
                    }
                }
                Err(e) => {
                    tracing::warn!(project.guid = %guid, why = %e, "another device's edit did not reach the engine");
                }
            }
        }
    })
}

/// Runs `apply` after edits arrive in `doc` from elsewhere (an import,
/// never a local commit); a burst of them is one run.
fn watch_imports<F, Fut>(
    doc: &engine_sync::loro::LoroDoc,
    apply: F,
) -> engine_sync::loro::Subscription
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    let subscription = doc.subscribe_root(Arc::new(move |event| {
        if matches!(
            event.triggered_by,
            engine_sync::loro::EventTriggerKind::Import
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

/// What an edit that reached a song from elsewhere is named in its file
/// when it came with no name.
const REMOTE_LABEL: &str = "Remote edits";

/// What the edits that reached a song from elsewhere are named in its
/// history: the steps they were where they were made (`Agent: tighten
/// snare`; `Set volume`), or [`REMOTE_LABEL`] when they came unnamed.
fn remote_label(names: &[String]) -> String {
    if names.is_empty() {
        REMOTE_LABEL.to_owned()
    } else {
        names.join("; ")
    }
}

/// How many updates a branch keeps before a save folds them into one
/// snapshot ([`Store::compact`]) whatever else happens. An open replays
/// every update since the last snapshot, working the song out again from
/// each: Unbreakable's 377 took 16 s to open, its snapshot 0.2 s.
const COMPACT_AFTER: usize = 64;

/// How long the song is left alone before what it has done since its
/// snapshot is folded into a new one ([`compact_when_idle`]), when it has
/// done at least [`FOLD_AT_LEAST`] since. A fold of a long-edited song is
/// seconds of work (Unbreakable: 2 s holding the song, 7 s more behind
/// it): at 10 s it ran after nearly every pause in an edit session.
const IDLE: std::time::Duration = std::time::Duration::from_secs(60);

/// How many updates on its snapshot make a song worth folding after
/// [`IDLE`]: a handful replay quickly at an open.
const FOLD_AT_LEAST: usize = 8;

/// Left alone this long, a song is folded whatever it has done since.
const LONG_IDLE: std::time::Duration = std::time::Duration::from_secs(600);

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
                // The write last looked at, and whether that was at
                // LONG_IDLE: each write is looked at twice at most, not
                // every second while a few updates wait for LONG_IDLE.
                let mut looked: Option<(web_time::Instant, bool)> = None;
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    let Some(at) = LAST_WRITE.lock().ok().and_then(|at| *at) else {
                        continue;
                    };
                    let idle = at.elapsed();
                    let long = idle >= LONG_IDLE;
                    if idle < IDLE || looked.is_some_and(|(seen, was_long)| seen == at && was_long == long) {
                        continue;
                    }
                    looked = Some((at, long));
                    // Its own trace: how long the fold held the songs.
                    let fold = tracing::info_span!("song.compact");
                    let _fold = fold.enter();
                    // Only the snapshot is taken with the songs held (it
                    // needs the doc as it is); packing and writing it, the
                    // slow part, is done after — an edit finishing
                    // meanwhile is recorded at once, not behind the fold.
                    let taken = run(async {
                        let mut songs = open_songs().lock().await;
                        let mut taken = Vec::new();
                        let due = |o: &&mut Open| {
                            o.uncompacted >= FOLD_AT_LEAST || (long && o.uncompacted > 0)
                        };
                        for open in songs.values_mut().filter(due) {
                            match take_fold(open) {
                                Ok(snapshot) => {
                                    open.uncompacted = 0;
                                    taken.push((Arc::clone(&open.store), snapshot));
                                }
                                Err(e) => tracing::warn!(song.error = %e, "the song's history could not be folded"),
                            }
                        }
                        // Nothing left to fold: quiet until the next write.
                        if songs.values().all(|o| o.uncompacted == 0)
                            && let Ok(mut at) = LAST_WRITE.lock()
                        {
                            *at = None;
                        }
                        taken
                    });
                    match taken {
                        Ok(taken) => {
                            for (store, snapshot) in taken {
                                write_snapshot_later(&store, snapshot, false);
                            }
                        }
                        Err(e) => tracing::warn!(song.error = %e, "folding the songs' history did not run"),
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

/// The file the open song `project` (its guid) was opened from.
#[must_use]
pub fn path_of(project: &str) -> Option<PathBuf> {
    run(async {
        open_songs()
            .lock()
            .await
            .get(project)
            .map(|o| o.path.clone())
    })
    .ok()
    .flatten()
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
    // The plugin states no mix names any more go with this save (read
    // into the doc below, with the rest of the song).
    match crate::mix_states::sweep(
        daw,
        &daw_proto::ProjectContext::Project(project_guid.to_owned()),
    ) {
        Ok(dropped) => crate::telemetry::set(
            "save.mix_states_dropped",
            i64::try_from(dropped.len()).unwrap_or(i64::MAX),
        ),
        // Left for the next save: the song saves as it is.
        Err(e) => crate::telemetry::set("save.mix_states_error", e),
    }
    // Every step read is in the doc first.
    written(WRITE_WAIT);
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
        // A song opened as an alternative (`Song.session/Live`) and saved
        // into its own file with no branch named (SESSION_SAVE, a chart's
        // rebuild: they know the file, not the branch) saves to the
        // alternative it is. Saving it to the main branch wrote it over
        // the song as recorded.
        let branch = match songs.get(&guid) {
            Some(open)
                if open.path == path
                    && branch == session_format::store::MAIN
                    && open.branch != session_format::store::MAIN =>
            {
                open.branch.clone()
            }
            _ => branch,
        };
        let same = songs
            .get(&guid)
            .is_some_and(|open| open.path == path && open.branch == branch);
        if !same {
            // Into this file for the first time (or another file): its
            // history so far, the song written over it.
            // A copy saved elsewhere leaves the song's own bridge — and
            // what it knows of the song's ids — where it is.
            // A project has one bridge (`engine_sync::bridge::claim`).
            if songs.contains_key(&guid) {
                drop(songs);
                return save_copy(&daw, &guid, &path, &branch, &label)
                    .await
                    .map(|()| path);
            }
            let mut open = begin(&daw, &guid, &path, &branch).await?;
            open._remote = Some(watch_remote(&guid, open.bridge.doc().loro()));
            forget_undo(&guid);
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
    let doc = SessionDoc::from_loro(loaded);
    let saved = doc.version();
    let chart = doc.read().chart;
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
    let opened_at = bridge.doc().loro().oplog_frontiers();
    Ok(Open {
        path: path.to_path_buf(),
        branch: branch.to_owned(),
        store,
        bridge,
        saved,
        uncompacted,
        _remote: None,
        opened_at,
        archive: None,
    })
}

/// The song `guid` as it is now into the project file at `path` on
/// `branch`, once — a copy saved elsewhere. The engine is read afresh and
/// written over the branch, and the song keeps its own bridge (a second
/// would rewrite it against the first).
async fn save_copy(
    daw: &Standalone,
    guid: &str,
    path: &Path,
    branch: &str,
    label: &str,
) -> Result<(), String> {
    let (store, loaded, _) = open_branch(path, branch)?;
    let doc = SessionDoc::from_loro(loaded);
    let before = doc.version();
    let chart = doc.read().chart;
    let blobs = StoreBlobs::new(store.clone());
    let engine = direct(daw);
    engine_sync::bridge::write_once(
        &project(daw, guid).await?,
        &media_of(path),
        &doc,
        chart,
        blobs.as_ref(),
        Some(engine.as_ref()),
    )
    .await
    .map_err(|e| e.to_string())?;
    if doc.version() != before {
        let update = doc.updates_since(&before).map_err(|e| e.to_string())?;
        with_store(&store, |s| {
            s.append_update(branch, label, &update)
                .map_err(|e| e.to_string())
        })?;
    }
    with_store(&store, |s| s.checkpoint().map_err(|e| e.to_string()))
}

/// The song as it is now into its file: read, and what changed appended.
async fn save_open(open: &mut Open, label: &str) -> Result<(), String> {
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
    append_as(open, label, None)
}

/// A step this replica wrote into the song's doc (`write_read`), appended
/// as far as its own commit: what arrived from elsewhere after it (a room
/// member's edit, an agent's step made there) is left for
/// [`watch_remote`]'s append, its own moment under its own name. The doc
/// is written before the songs are held, so an import can land between
/// the write and this append; read whole, it was folded into this step's
/// moment, and the step made elsewhere was missing from the history.
fn append_step(open: &mut Open, label: &str, action: Option<&str>) -> Result<(), String> {
    let loro = open.bridge.doc().loro().clone();
    let now = loro.oplog_vv();
    let (upto, frontiers) = own_reach(&loro, &now, &open.saved)
        .unwrap_or_else(|| (now.clone(), loro.vv_to_frontiers(&now)));
    append_upto(open, (label, action), &upto, &frontiers)
}

/// The version and frontier of `loro` just after its own last commit, when
/// `now` (its version, read once) holds more than that past `saved` (an
/// import landed after it); `None` when there is nothing else, or no
/// commit of its own since.
fn own_reach(
    loro: &engine_sync::loro::LoroDoc,
    now: &engine_sync::loro::VersionVector,
    saved: &engine_sync::loro::VersionVector,
) -> Option<(
    engine_sync::loro::VersionVector,
    engine_sync::loro::Frontiers,
)> {
    let me = loro.peer_id();
    let end = now.get(&me).copied().unwrap_or(0);
    if end <= saved.get(&me).copied().unwrap_or(0) {
        return None;
    }
    let frontiers = engine_sync::loro::Frontiers::from_id(engine_sync::loro::ID::new(
        me,
        end.saturating_sub(1),
    ));
    let mut reach = loro.frontiers_to_vv(&frontiers)?;
    reach.merge(saved);
    (reach != *now).then_some((reach, frontiers))
}

/// [`append`], `action` the action that made the step, kept in the song's
/// history ([`history`]) with its label and frontier.
fn append_as(open: &mut Open, label: &str, action: Option<&str>) -> Result<(), String> {
    let loro = open.bridge.doc().loro().clone();
    let now = loro.oplog_vv();
    // The state the history goes back to: the doc's own, which is the
    // oplog's unless the doc is checked out at another version.
    let frontiers = if loro.is_detached() {
        loro.state_frontiers()
    } else {
        loro.vv_to_frontiers(&now)
    };
    append_upto(open, (label, action), &now, &frontiers)
}

/// The doc from the last save up to `upto` (at `frontiers`) appended to the
/// song's branch as the step `label`, and `saved` moved there. The version
/// is read once by the caller: what is imported while the store is written
/// (a slow disk under load) is past `upto`, left for the next append under
/// its own name, never exported under this one.
fn append_upto(
    open: &mut Open,
    (label, action): (&str, Option<&str>),
    upto: &engine_sync::loro::VersionVector,
    frontiers: &engine_sync::loro::Frontiers,
) -> Result<(), String> {
    if *upto != open.saved {
        // Every step in the history: a name, a time and the frontier to go
        // back to, never folded away.
        with_store(&open.store, |s| {
            s.note(
                &open.branch,
                (session_format::store::MomentKind::Step, label, action),
                frontiers,
            )
            .map(drop)
            .map_err(|e| e.to_string())
        })?;
        let spans: Vec<engine_sync::loro::IdSpan> = upto.sub_iter(&open.saved).collect();
        let update = open
            .bridge
            .doc()
            .loro()
            .export(engine_sync::loro::ExportMode::updates_in_range(spans))
            .map_err(|e| e.to_string())?;
        with_store(&open.store, |s| {
            s.append_update(&open.branch, label, &update)
                .map_err(|e| e.to_string())
        })?;
        open.saved = upto.clone();
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
        let snapshot = take_fold(open)?;
        open.uncompacted = 0;
        // Packed and written on a thread of its own: the edit that tipped it
        // over is recorded now, not after the fold.
        write_snapshot_later(&open.store, snapshot, true);
    }
    Ok(())
}

/// Pack `snapshot` and make it its branch's base in `store` — here, or
/// (`spawned`) on a thread of its own — the song itself not held.
fn write_snapshot_later(
    store: &Arc<Mutex<Store>>,
    snapshot: session_format::store::Snapshot,
    spawned: bool,
) {
    let store = Arc::clone(store);
    let span = tracing::Span::current();
    let work = move || {
        let _in = span.enter();
        let _job = crate::jobs::start("Saving the song's history", 0);
        let stage = tracing::info_span!("song.compact").entered();
        let written = snapshot
            .pack()
            .map_err(|e| e.to_string())
            .and_then(|packed| {
                with_store(&store, |s| {
                    s.write_snapshot(packed).map_err(|e| e.to_string())
                })
            });
        drop(stage);
        if let Err(e) = written {
            tracing::warn!(song.error = %e, "the song's history could not be folded");
        }
    };
    if !spawned {
        work();
        return;
    }
    if let Err(e) = std::thread::Builder::new()
        .name("song-compact-write".into())
        .spawn(work)
    {
        tracing::warn!(song.error = %e, "no thread to fold the song's history on");
    }
}

/// Record each edit step the engine finishes into its song's file, once
/// that song is open from it — on a thread of its own, so the window
/// never waits on a read of the engine. Steps that finish while one is
/// being recorded are read together, as one commit named for them all.
/// Steps the engine finished that the recorder has not read yet (once
/// read, what the engine does next is not in their commit, though the
/// doc's write of them may still run: [`WRITING`]).
static PENDING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// [`PENDING`], song by song: what a song's own undo and going back read —
/// another song's step waiting to be recorded is not one of its own.
fn pending_songs() -> &'static Mutex<HashMap<String, usize>> {
    static SONGS: std::sync::OnceLock<Mutex<HashMap<String, usize>>> = std::sync::OnceLock::new();
    SONGS.get_or_init(Mutex::default)
}

/// Whether the song `guid` has steps the recorder has not recorded yet.
fn song_pending(guid: &str) -> bool {
    pending_songs()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(guid)
        .is_some_and(|n| *n > 0)
}

/// Steps read whose doc write is still running on the writer thread —
/// changed with the songs held, so what holds them sees it settled.
static WRITING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// The writes sent to the writer, numbered in order, and the last done.
static SENT_WRITES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A mark asked for while a step's write ran: noted at the doc's frontier
/// once the writes sent before it are done, before any after it.
struct QueuedMark {
    guid: String,
    name: String,
    action: String,
    after: u64,
}

fn queued_marks() -> &'static Mutex<Vec<QueuedMark>> {
    static QUEUED: std::sync::LazyLock<Mutex<Vec<QueuedMark>>> =
        std::sync::LazyLock::new(Default::default);
    &QUEUED
}

/// A read step for the writer: its song, name and action, what was read,
/// its number, and the span it was recorded under.
struct Write {
    guid: String,
    label: String,
    action: Option<String>,
    read: engine_sync::LocalRead,
    writer: engine_sync::StepWriter,
    seq: u64,
    span: tracing::Span,
}

/// Where read steps go to be written into their songs' docs, in order, on
/// a thread of their own: a write of a big edit (seconds for the whole
/// kit's items) never holds the songs, so the next step is read and the
/// next action marked meanwhile.
fn writes() -> &'static Mutex<std::sync::mpsc::Sender<Write>> {
    static WRITES: std::sync::LazyLock<Mutex<std::sync::mpsc::Sender<Write>>> =
        std::sync::LazyLock::new(|| {
            let (tx, rx) = std::sync::mpsc::channel::<Write>();
            let spawned = std::thread::Builder::new()
                .name("project-file-writes".into())
                .spawn(move || {
                    for job in rx {
                        write_read(job);
                    }
                });
            if let Err(e) = spawned {
                tracing::warn!(song.error = %e, "no thread to write the song's steps on");
            }
            Mutex::new(tx)
        });
    &WRITES
}

/// Write one read step into its song's doc (nothing held), then — the
/// songs held — append it to the file and note the marks that waited on
/// it.
fn write_read(job: Write) {
    let span = tracing::info_span!(
        parent: &job.span,
        "song.write",
        steps.label = job.label.as_str(),
        steps.marks = tracing::field::Empty,
    );
    let _in = span.enter();
    let wrote = job.writer.write(&job.read, &job.label);
    let done = run(async {
        let mut songs = open_songs().lock().await;
        if let Err(e) = &wrote {
            tracing::warn!(project.guid = %job.guid, why = %e, "an edit step was not written to the song's doc");
        } else if let Some(open) = songs.get_mut(&job.guid) {
            let appended = tracing::info_span!("project_file.append")
                .in_scope(|| append_step(open, &job.label, job.action.as_deref()));
            if let Err(why) = appended {
                tracing::warn!(project.guid = %job.guid, why, "an edit step was not appended to the song's file");
            }
        }
        let noted = note_queued(&mut songs, job.seq);
        span.record("steps.marks", noted);
        WRITING.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        drop(songs);
    });
    if let Err(why) = done {
        WRITING.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        tracing::warn!(why, "an edit step's append did not run");
    }
}

/// Note every queued mark that waited on the writes up to `done`, at its
/// song's frontier now. How many.
fn note_queued(songs: &mut HashMap<String, Open>, done: u64) -> usize {
    let due: Vec<QueuedMark> = match queued_marks().lock() {
        Ok(mut queued) => {
            let (due, wait) = std::mem::take(&mut *queued)
                .into_iter()
                .partition(|m| m.after <= done);
            *queued = wait;
            due
        }
        Err(_) => Vec::new(),
    };
    let mut noted = 0;
    for m in due {
        let Some(open) = songs.get_mut(&m.guid) else {
            continue;
        };
        match note_mark(open, &m.name, &m.action) {
            Ok(()) => noted += 1,
            Err(why) => {
                tracing::warn!(project.guid = %m.guid, why, "a mark was not noted on the song");
            }
        }
    }
    noted
}

/// Note the mark `name` (set by `action`) at the doc's frontier now.
fn note_mark(open: &Open, name: &str, action: &str) -> Result<(), String> {
    let frontiers = open.bridge.doc().loro().state_frontiers();
    with_store(&open.store, |s| {
        s.note(
            &open.branch,
            (session_format::store::MomentKind::Mark, name, Some(action)),
            &frontiers,
        )
        .map(drop)
        .map_err(|e| e.to_string())
    })
}

/// Wait (at most `most`) until no step's doc write is running: what reads
/// or rewrites the doc whole (an undo, a restore, a copy of the file)
/// then sees every step recorded. Whether it got there.
pub fn written(most: std::time::Duration) -> bool {
    if WRITING.load(std::sync::atomic::Ordering::SeqCst) == 0 {
        return true;
    }
    let span = tracing::info_span!("history.written", history.waited_ms = tracing::field::Empty);
    let _in = span.enter();
    let began = web_time::Instant::now();
    let mut nap = std::time::Duration::from_micros(200);
    while WRITING.load(std::sync::atomic::Ordering::SeqCst) > 0 && began.elapsed() < most {
        std::thread::sleep(nap);
        nap = (nap * 2).min(std::time::Duration::from_millis(5));
    }
    span.record("history.waited_ms", began.elapsed().as_secs_f64() * 1e3);
    WRITING.load(std::sync::atomic::Ordering::SeqCst) == 0
}

/// The engine's revision of the song `guid` when its bridge last read it
/// whole (sampled just before the read, so an edit during it counts as
/// after).
fn read_revisions() -> &'static Mutex<HashMap<String, u64>> {
    static READ: std::sync::LazyLock<Mutex<HashMap<String, u64>>> =
        std::sync::LazyLock::new(Default::default);
    &READ
}

/// The song `guid`'s revision in the engine now: every write moves it on.
/// `None`: no engine in this process to ask.
fn revision(guid: &str) -> Option<u64> {
    crate::open::local_engine()?.read_project(guid, |p| p.revision)
}

/// The song `guid` was read at `revision`.
fn read_at(guid: &str, revision: Option<u64>) {
    if let (Some(revision), Ok(mut read)) = (revision, read_revisions().lock()) {
        read.insert(guid.to_owned(), revision);
    }
}

/// Whether the engine has written nothing to the song `guid` since its
/// bridge last read it — so a mark needs no read of it (a whole song's
/// read: a quarter second on a sliced kit). False when it cannot be told.
fn unchanged_since_read(guid: &str) -> bool {
    let Some(now) = revision(guid) else {
        return false;
    };
    read_revisions()
        .lock()
        .ok()
        .and_then(|read| read.get(guid).copied())
        == Some(now)
}

/// Hand the step `label` of the song `guid` (made by `action`), read off
/// its engine, to the writer — the songs held, so its number is in order.
fn send_write(
    open: &Open,
    (guid, label, action): (&str, &str, Option<&str>),
    read: engine_sync::LocalRead,
) -> Result<(), String> {
    let seq = SENT_WRITES.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    WRITING.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let job = Write {
        guid: guid.to_owned(),
        label: label.to_owned(),
        action: action.map(str::to_owned),
        read,
        writer: open.bridge.writer(),
        seq,
        span: tracing::Span::current(),
    };
    let sent = writes()
        .lock()
        .map_err(|_| "the writer's lock".to_owned())
        .and_then(|tx| tx.send(job).map_err(|e| e.to_string()));
    if let Err(why) = sent {
        WRITING.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        return Err(format!("the step was read but not written: {why}"));
    }
    Ok(())
}

/// The longest an undo, a restore or a copy waits for a step's write.
pub const WRITE_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// How the kit's MIDI agrees with its hits after `labels` (the steps
/// recorded together): an undo or a redo puts both back, so the views are
/// written from the hits; any other step may be an edit of a view.
fn sync_mode(labels: &[String]) -> crate::assembly::StepSync {
    if labels
        .iter()
        .any(|l| l.starts_with("Undo ") || l.starts_with("Redo "))
    {
        crate::assembly::StepSync::Follow
    } else {
        crate::assembly::StepSync::Reconcile
    }
}

pub fn record_steps() {
    compact_when_idle();
    static LISTENING: std::sync::Once = std::sync::Once::new();
    LISTENING.call_once(|| {
        // Each step with the span it finished in (the action that made
        // it): its recording is traced under that.
        // And the action that made it, read here, on its thread.
        type Finished = (String, String, Option<u64>, (tracing::Span, Option<String>));
        let (tx, rx) = std::sync::mpsc::channel::<Finished>();
        let tx = Mutex::new(tx);
        daw::standalone::steps::listen(Arc::new(move |project, label| {
            // A CLAP's `hits:` subscriptions follow each step.
            // and the song's revision: a step no watched action made (a drag).
            if let Some(engine) = crate::assembly::get() {
                (engine.step_finished)(label);
            }
            // Counted before it is sent: the recorder may take it and count
            // it off before this thread runs again (a loaded machine), and a
            // count made after that was never taken off — the song read as
            // pending for good, every undo after it reading a "pending" step
            // that changed nothing and undoing that, by the engine's last
            // name, instead of the step (CI: "undid Set track").
            let Ok(tx) = tx.lock() else { return };
            *pending_songs()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .entry(project.to_owned())
                .or_default() += 1;
            PENDING.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let sent = tx.send((
                project.to_owned(),
                label.to_owned(),
                crate::history::running_run(),
                (tracing::Span::current(), crate::history::running_action()),
            ));
            if sent.is_err() {
                PENDING.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                let mut songs = pending_songs()
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(left) = songs.get_mut(project) {
                    *left = left.saturating_sub(1);
                    if *left == 0 {
                        songs.remove(project);
                    }
                }
            }
        }));
        let _ = std::thread::Builder::new()
            .name("project-file-steps".into())
            .spawn(move || {
                while let Ok(first) = rx.recv() {
                    // Steps of one action call are one commit; another
                    // call's are its own (`history_marks::gather`).
                    let finished: Vec<Finished> =
                        std::iter::once(first).chain(rx.try_iter()).collect();
                    let mut taken = finished.len();
                    let mut by_song: HashMap<String, usize> = HashMap::new();
                    for (project, ..) in &finished {
                        *by_song.entry(project.clone()).or_default() += 1;
                    }
                    let steps = crate::history_marks::gather(finished);
                    for (project, labels, (made_in, action)) in steps {
                        // The steps' recording, under the action that made
                        // the first of them (it may outlast it).
                        let step = tracing::info_span!(
                            parent: &made_in,
                            "song.step",
                            steps.labels = labels.len(),
                            steps.label = labels.first().map(String::as_str).unwrap_or_default(),
                        );
                        let _step = step.enter();
                        // What is made from the recording follows it first —
                        // the kit's samples onto where its hits play now — so
                        // it is in the same commit as the edit that moved it.
                        // What the follow read of the views (`hits_midi::sync`
                        // records it on the span it runs in) is on its stage.
                        let stage = tracing::info_span!(
                            "step.follow",
                            steps.views_edited = tracing::field::Empty,
                            steps.hits_added = tracing::field::Empty,
                            steps.hits_moved = tracing::field::Empty,
                            steps.hits_removed = tracing::field::Empty,
                        )
                        .entered();
                        if let Some(daw) = crate::open::local_engine()
                            && let Some(engine) = crate::assembly::get()
                        {
                            (engine.samples_follow)(
                                daw,
                                &daw::service::ProjectContext::Project(project.clone()),
                                sync_mode(&labels),
                            );
                        }
                        drop(stage);
                        let stage = tracing::info_span!("step.record").entered();
                        if let Err(why) = record(&project, &labels.join("; "), action.as_deref()) {
                            tracing::warn!(project.guid = %project, why, "an edit step was not recorded");
                        }
                        drop(stage);
                        // Read: what the next action does no longer reaches
                        // these steps' commit (its write runs on).
                        PENDING.fetch_sub(labels.len(), std::sync::atomic::Ordering::SeqCst);
                        taken = taken.saturating_sub(labels.len());
                    }
                    {
                        let mut songs = pending_songs()
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        for (project, n) in by_song {
                            if let Some(left) = songs.get_mut(&project) {
                                *left = left.saturating_sub(n);
                                if *left == 0 {
                                    songs.remove(&project);
                                }
                            }
                        }
                    }
                    PENDING.fetch_sub(taken, std::sync::atomic::Ordering::SeqCst);
                }
            });
    });
}

/// Run `work` on the store of the song `guid` and the file it is open
/// from, holding the song: no step is appended to the file while it runs
/// (what makes `SESSION_BACKUP`'s copy consistent).
///
/// # Errors
/// The song is not open from its project file, or `work` failed.
pub fn with_open_store<T: Send>(
    guid: &str,
    work: impl FnOnce(&Store, &Path) -> Result<T, String> + Send,
) -> Result<T, String> {
    let guid = guid.to_owned();
    written(WRITE_WAIT);
    run(async move {
        let songs = open_songs().lock().await;
        let open = songs
            .get(&guid)
            .ok_or("the song is not open from its project file (a .session)")?;
        let path = open.path.clone();
        let done = with_store(&open.store, |s| work(s, &path));
        drop(songs);
        done
    })?
}

/// `read` the doc of the song `guid`, open from its project file, as its
/// bridge holds it, once every step read is in it: what the render worker's
/// copy of the song is synced from (`crate::render_worker`).
///
/// # Errors
/// The song is not open from its project file.
#[cfg(feature = "native")]
pub fn with_song_doc<T: Send>(
    guid: &str,
    read: impl FnOnce(&engine_sync::loro::LoroDoc) -> T + Send,
) -> Result<T, String> {
    let guid = guid.to_owned();
    written(WRITE_WAIT);
    run(async move {
        let songs = open_songs().lock().await;
        let open = songs
            .get(&guid)
            .ok_or("the song is not open from its project file (a .session)")?;
        Ok(read(open.bridge.doc().loro()))
    })?
}

/// How many steps the engine finished that the recorder has not recorded
/// yet: what a call waits on to be a step of its own
/// (`history_marks::settle`).
#[must_use]
pub fn steps_pending() -> usize {
    PENDING.load(std::sync::atomic::Ordering::SeqCst)
}

/// The engine finished the step `label` in the song `project_guid`: read
/// it into the song's doc as one commit and append that to its file — if
/// the song is open from one.
pub fn record(project_guid: &str, label: &str, action: Option<&str>) -> Result<(), String> {
    let guid = project_guid.to_owned();
    // A step made while an agent's undo step is open is named for it, in
    // the doc too: every device with the song (a room's members) reads it
    // as the agent's step.
    let label =
        with_book(&guid, |b| b.open_name().map(str::to_owned)).unwrap_or_else(|| label.to_owned());
    let action = action.map(str::to_owned);
    // On the project file's runtime, still under the step's span.
    run(tracing::Instrument::in_current_span(async move {
        let mut songs = open_songs().lock().await;
        let Some(open) = songs.get_mut(&guid) else {
            return Ok(());
        };
        let revision = revision(&guid);
        let read = tracing::Instrument::instrument(
            open.bridge.read_step(&label),
            tracing::info_span!("bridge.read_step"),
        )
        .await
        .map_err(|e| e.to_string())?;
        read_at(&guid, revision);
        // One undo item more in the doc, a quiet one included.
        with_book(&guid, |b| b.recorded(&label));
        let Some(read) = read else {
            return Ok(());
        };
        send_write(open, (&guid, &label, action.as_deref()), read)?;
        drop(songs);
        Ok(())
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
    // The doc holding every step read, the one undone among them.
    written(WRITE_WAIT);
    run(async move {
        let mut songs = open_songs().lock().await;
        let Some(open) = songs.get_mut(&guid) else {
            return Ok(Undone::NotOpen);
        };
        let done = step_back(open, (&guid, &pending), back).await;
        drop(songs);
        done
    })?
}

async fn step_back(
    open: &mut Open,
    (guid, pending): (&str, &str),
    back: bool,
) -> Result<Undone, String> {
    // A step of this song the recorder has not reached yet is recorded
    // first, so it is the one undone; with none waiting, nothing to read.
    if song_pending(guid) {
        if open
            .bridge
            .local_step(pending)
            .await
            .map_err(|e| e.to_string())?
        {
            append(open, pending)?;
        }
        with_book(guid, |b| b.recorded(pending));
    }
    // An agent's script on top: every one of its items, as one step.
    let group = with_book(guid, |b| {
        if back { b.to_undo() } else { b.to_redo() }.cloned()
    });
    if let Some(group) = group {
        let mut done = 0;
        for want in undo_groups::plan(&group, back) {
            let next = if back {
                open.bridge.next_undo()
            } else {
                open.bridge.next_redo()
            };
            if next.as_deref() != Some(want.as_str()) {
                if done == 0 {
                    break;
                }
                tracing::warn!(
                    project.guid = %guid,
                    undo.group = %group.name,
                    undo.done = done,
                    "an agent's step was taken back only in part: the song's undo is not what its count said"
                );
                break;
            }
            if one_back(open, back).await?.is_none() {
                break;
            }
            done += 1;
        }
        if done > 0 {
            with_book(guid, |b| if back { b.undone(done) } else { b.redone(done) });
            return Ok(Undone::Step(group.name));
        }
    }
    let step = one_back(open, back).await?;
    with_book(guid, |b| {
        if step.is_some() {
            if back { b.undone(1) } else { b.redone(1) }
        }
    });
    Ok(step.map_or(Undone::Nothing, Undone::Step))
}

/// Undo (`back`) or redo one undo item of the song's doc, the engine made
/// to match and it appended to the file: its name, `None` with nothing to
/// take back.
async fn one_back(open: &mut Open, back: bool) -> Result<Option<String>, String> {
    let step = if back {
        open.bridge.undo().await
    } else {
        open.bridge.redo().await
    }
    .map_err(|e| e.to_string())?;
    let Some(name) = step else {
        return Ok(None);
    };
    append(
        open,
        &format!("{} {name}", if back { "Undo" } else { "Redo" }),
    )?;
    Ok(Some(name))
}

// ── many steps undone as one (`undo_groups`) ────────────────────────────

/// Each song's undo items as this engine counted them, and its groups.
fn books() -> &'static Mutex<HashMap<String, undo_groups::Book>> {
    static BOOKS: std::sync::LazyLock<Mutex<HashMap<String, undo_groups::Book>>> =
        std::sync::LazyLock::new(Default::default);
    &BOOKS
}

fn with_book<T>(guid: &str, f: impl FnOnce(&mut undo_groups::Book) -> T) -> T {
    let mut books = books()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    f(books.entry(guid.to_owned()).or_default())
}

/// A new bridge on the song `guid`: its doc's undo starts empty.
fn forget_undo(guid: &str) {
    books()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(guid);
}

/// From now, every step of the song `guid` the recorder reads is the
/// undo step `name`'s, until [`group_end`]: `SESSION_UNDO` takes them
/// back as one. The steps before it are read first, so none of them is
/// in it. False when the song is not open from its project file (its
/// engine's own undo has no groups).
#[must_use]
pub fn group_begin(guid: &str, name: &str) -> bool {
    if !is_open(guid) {
        return false;
    }
    crate::history_marks::settle(crate::history_marks::SETTLE);
    with_book(guid, |b| b.begin(name));
    true
}

/// End the undo step `name` of the song `guid` (its steps read first):
/// how many of the song's steps it holds, when it is open and made any.
#[must_use]
pub fn group_end(guid: &str, name: &str) -> Option<usize> {
    crate::history_marks::settle(crate::history_marks::SETTLE);
    with_book(guid, |b| b.end(name)).map(|g| g.labels.len())
}

/// Drop the undo step `name` of the song `guid` unfinished, its steps
/// left steps of their own (its agent went away). Whether it was open.
pub fn group_abandon(guid: &str, name: &str) -> bool {
    with_book(guid, |b| b.abandon(name))
}

/// Whether the undo step `name` is being made on the song `guid`.
#[must_use]
pub fn group_open(guid: &str, name: &str) -> bool {
    with_book(guid, |b| b.is_open(name))
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
    // Every step read is in the doc first.
    written(WRITE_WAIT);
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
    let began = web_time::Instant::now();
    let step = tracing::info_span!("load.store").entered();
    let (store, loaded, uncompacted) =
        tracing::info_span!("project_file.open_branch").in_scope(|| open_branch(path, branch))?;
    drop(step);
    span.record("load.store_ms", began.elapsed().as_secs_f64() * 1e3);
    let began = web_time::Instant::now();
    let step = tracing::info_span!("load.doc").entered();
    span.record("load.uncompacted", uncompacted);
    // A backlog on the snapshot is folded once the song is left alone.
    if uncompacted > 0
        && let Ok(mut at) = LAST_WRITE.lock()
    {
        *at = Some(web_time::Instant::now());
    }
    let doc =
        tracing::info_span!("project_file.from_loro").in_scope(|| SessionDoc::from_loro(loaded));
    let saved = doc.version();
    drop(step);
    span.record("load.doc_ms", began.elapsed().as_secs_f64() * 1e3);
    let began = web_time::Instant::now();
    let step = tracing::info_span!("load.build").entered();
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
    drop(step);
    span.record("load.build_ms", began.elapsed().as_secs_f64() * 1e3);
    let began = web_time::Instant::now();
    let step = tracing::info_span!("load.land").entered();
    let daw = daw.clone();
    let path = path.to_path_buf();
    let branch = branch.to_owned();
    let made = guid.clone();
    run(async move {
        // On the project file's runtime: the song's transport lives on it.
        stage.land(&made).map_err(|e| e.to_string())?;
        let bridge = built
            .moved_to(project(&daw, &made).await?, Some(direct(&daw)))
            .map_err(|e| e.to_string())?;
        let remote = Some(watch_remote(&made, bridge.doc().loro()));
        forget_undo(&made);
        let opened_at = bridge.doc().loro().oplog_frontiers();
        open_songs().lock().await.insert(
            made,
            Open {
                path,
                branch,
                store,
                bridge,
                saved,
                uncompacted,
                _remote: remote,
                opened_at,
                archive: None,
            },
        );
        Ok::<(), String>(())
    })??;
    drop(step);
    span.record("load.land_ms", began.elapsed().as_secs_f64() * 1e3);
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
    // Every step read is in the doc first.
    written(WRITE_WAIT);
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\']) || name == session_format::store::MAIN {
        return Err(format!("{name:?} is not a name an alternative can have"));
    }
    let (file, branch) = save_target(path);
    // A checkpoint named for what it comes before already says so.
    let label = if name.starts_with("Before ") {
        name.to_owned()
    } else {
        format!("Before {name}")
    };
    save(daw, project_guid, &file, &branch, &label)?;
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

/// The song `guid`'s steps and marks, newest first — `limit` from
/// `offset` — each with whether its version is still in the doc's history
/// to go back to; and how many there are.
///
/// # Errors
/// The song is not open from its file, or the file refused.
pub fn history(guid: &str, page: (u32, u32)) -> Result<(Vec<(Moment, bool)>, u32), String> {
    // Every step read is in the doc first.
    written(WRITE_WAIT);
    let guid = guid.to_owned();
    run(async move {
        let songs = open_songs().lock().await;
        let open = songs
            .get(&guid)
            .ok_or("the song is not open from its file")?;
        let (moments, total, archived) = with_store(&open.store, |s| {
            let (moments, total) = s.history(&open.branch, page).map_err(|e| e.to_string())?;
            let archived = s
                .archived_through(&open.branch)
                .map_err(|e| e.to_string())?;
            Ok((moments, total, archived))
        })?;
        let vv = open.bridge.doc().loro().oplog_vv();
        // Reachable in the song's own history, or (before its shallow root)
        // in its archive.
        let out = moments
            .into_iter()
            .map(|m| {
                let reachable =
                    reachable(&vv, &m.frontiers) || archived.is_some_and(|last| m.seq <= last);
                (m, reachable)
            })
            .collect();
        drop(songs);
        Ok((out, total))
    })?
}

/// Whether every op `frontiers` (encoded) names is in the history `vv`
/// holds: a version of this doc's lineage, to go back to.
fn reachable(vv: &engine_sync::loro::VersionVector, frontiers: &[u8]) -> bool {
    engine_sync::loro::Frontiers::decode(frontiers)
        .is_ok_and(|f| f.iter().all(|id| vv.includes_id(id)))
}

/// Set a mark on the song `guid` as it is now — its doc's frontier and a
/// name, a few bytes in its file, no branch — named by `name_for` from the
/// marks it has already. The steps before it are read first (never their
/// writes waited on): while one is still being written into the doc, the
/// mark is queued and the writer notes it right after, before any step
/// read since. Whatever the engine holds that no step read in time is
/// recorded first, as a step named for the mark, so the mark holds the
/// song as it is. Says the mark's name.
///
/// # Errors
/// The song is not open from its file, or the engine would not be read,
/// or the file refused.
pub fn mark(
    guid: &str,
    action: &str,
    name_for: impl FnOnce(&[String]) -> String + Send,
) -> Result<String, String> {
    let span = tracing::info_span!(
        "history.mark_set",
        history.queued = tracing::field::Empty,
        history.read_first = tracing::field::Empty,
    );
    let _in = span.enter();
    // What it marks is the song as the steps before it left it: those
    // read, never their (slow) writes waited on.
    crate::history_marks::settle(crate::history_marks::SETTLE);
    let guid = guid.to_owned();
    let action = action.to_owned();
    run(tracing::Instrument::in_current_span(async move {
        let mut songs = open_songs().lock().await;
        let open = songs
            .get_mut(&guid)
            .ok_or("the song is not open from its project file (a .session)")?;
        let mut taken = with_store(&open.store, |s| {
            s.mark_names(&open.branch).map_err(|e| e.to_string())
        })?;
        if let Ok(queued) = queued_marks().lock() {
            taken.extend(
                queued
                    .iter()
                    .filter(|m| m.guid == guid)
                    .map(|m| m.name.clone()),
            );
        }
        let name = name_for(&taken);
        // Steps not read in time, or an edit no step recorded: read first,
        // as a step named for the mark, so the mark holds the song as it
        // is. An engine unchanged since its last read is not read.
        let unread =
            PENDING.load(std::sync::atomic::Ordering::SeqCst) > 0 || !unchanged_since_read(&guid);
        tracing::Span::current().record("history.read_first", unread);
        if unread {
            let revision = revision(&guid);
            if let Some(read) = open
                .bridge
                .read_step(&name)
                .await
                .map_err(|e| e.to_string())?
            {
                send_write(open, (&guid, &name, Some(&action)), read)?;
            }
            read_at(&guid, revision);
        }
        // A step still being written (its own, or one before it): the mark
        // goes after it, noted by the writer — before any step read after
        // this. Never waited on.
        let writing = WRITING.load(std::sync::atomic::Ordering::SeqCst) > 0;
        tracing::Span::current().record("history.queued", writing);
        if writing && let Ok(mut queued) = queued_marks().lock() {
            queued.push(QueuedMark {
                guid: guid.clone(),
                name: name.clone(),
                action: action.clone(),
                after: SENT_WRITES.load(std::sync::atomic::Ordering::SeqCst),
            });
            drop(queued);
            drop(songs);
            return Ok(name);
        }
        note_mark(open, &name, &action)?;
        drop(songs);
        Ok(name)
    }))?
}

/// Bring the song `guid` back to the step or mark `which` of its history
/// (`SESSION_HISTORY_RESTORE`): the doc forked at its frontier and read, and
/// that written over the song as one new step (`label` names it) — undone
/// like any other, nothing of the history rewritten — the engine made to
/// match and the step appended to the file. Says what it went back to.
///
/// # Errors
/// No such step or mark, a version no longer in the doc's history (a live
/// set joined since), or the engine would not be read or made to match.
pub fn restore(guid: &str, which: MomentRef) -> Result<Moment, String> {
    let guid = guid.to_owned();
    written(WRITE_WAIT);
    run(async move {
        let mut songs = open_songs().lock().await;
        let open = songs
            .get_mut(&guid)
            .ok_or("the song is not open from its project file (a .session)")?;
        let moment = with_store(&open.store, |s| {
            s.moment(&open.branch, &which).map_err(|e| e.to_string())
        })?
        .ok_or_else(|| match &which {
            MomentRef::Seq(seq) => {
                format!("the song has no step {seq} (SESSION_HISTORY lists them)")
            }
            MomentRef::Mark(name) => {
                format!("the song has no mark {name:?} (SESSION_HISTORY lists them)")
            }
        })?;
        let doc = open.bridge.doc().loro().clone();
        let at =
            engine_sync::loro::Frontiers::decode(&moment.frontiers).map_err(|e| e.to_string())?;
        // From the song's own history (this session's, since its shallow
        // root), else from its archive, which holds everything before.
        let here = reachable(&doc.oplog_vv(), &moment.frontiers)
            .then(|| session_format::store::fork_at(&doc, &at).ok())
            .flatten();
        let past = match here {
            Some(past) => past,
            None => {
                let archived = with_store(&open.store, |s| {
                    s.archived_through(&open.branch).map_err(|e| e.to_string())
                })?;
                if !archived.is_some_and(|last| moment.seq <= last) {
                    return Err(format!(
                        "{:?} is of a version no longer in the song's history (it joined a live set since): open the version kept aside then",
                        moment.label
                    ));
                }
                let span = tracing::info_span!("history.from_archive");
                let _in = span.enter();
                let archive = open.archive()?;
                let whole = with_store(&archive, |a| {
                    a.load(&open.branch).map_err(|e| e.to_string())
                })?;
                session_format::store::fork_at(&whole, &at).map_err(|e| e.to_string())?
            }
        };
        let model = SessionDoc::from_loro(past).read();
        // A step of this song the recorder has not reached yet is recorded
        // first, so the going back is a step of its own after it.
        if song_pending(&guid) {
            if open
                .bridge
                .local_step("Edits")
                .await
                .map_err(|e| e.to_string())?
            {
                append(open, "Edits")?;
            }
        }
        let label = format!("Back to {}", moment.label);
        open.bridge
            .restore(model, &label)
            .await
            .map_err(|e| e.to_string())?;
        append_as(open, &label, Some("SESSION_HISTORY_RESTORE"))?;
        drop(songs);
        Ok(moment)
    })?
}

/// Let go of the branches `names` of the file the song `guid` is open
/// from, through its own store: nothing of the song itself, nor of a
/// branch not named.
///
/// # Errors
/// The song is not open from its file, or the file refused.
pub fn delete_branches(guid: &str, names: &[String]) -> Result<(), String> {
    // Every step read is in the doc first.
    written(WRITE_WAIT);
    let guid = guid.to_owned();
    let names = names.to_vec();
    run(async move {
        let songs = open_songs().lock().await;
        let open = songs
            .get(&guid)
            .ok_or("the song is not open from its file")?;
        let deleted = with_store(&open.store, |s| {
            names
                .iter()
                .try_for_each(|name| s.delete_branch(name))
                .and_then(|()| s.checkpoint())
                .map_err(|e| e.to_string())
        });
        drop(songs);
        deleted
    })?
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
pub fn doc_of(path: &Path) -> Option<engine_sync::loro::LoroDoc> {
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
    // Every step read is in the doc first.
    written(WRITE_WAIT);
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
            None => {
                let engine = direct(&daw_for);
                engine_sync::bridge::write_once(
                    &project(&daw_for, &live_guid).await?,
                    &media_of(&file),
                    &SessionDoc::new(),
                    saved.chart.clone(),
                    &engine_sync::engine::MemoryBlobs::default(),
                    Some(engine.as_ref()),
                )
                .await
                .map_err(|e| e.to_string())?
            }
        };
        Ok::<_, String>((reopened, differences(&live, &saved)))
    })??;
    let _ = daw::service::Projects::close(daw, &apart.0);
    Ok(apart.1)
}

/// The song `guid` as its own bridge reads it now, if it has one.
async fn live_read(guid: &str) -> Result<Option<engine_sync::SessionModel>, String> {
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
fn differences(live: &engine_sync::SessionModel, saved: &engine_sync::SessionModel) -> Vec<String> {
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
                        let chain = |fx: &[engine_sync::FxState]| {
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
                    let lane = |l: &engine_sync::LaneState| {
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

// ── A live set's doc of a song ─────────────────────────────────────────
//
// A song has one doc, its file's, and one bridge to the engine. A live
// set keeps a doc of the song too; it is kept in step with the song's doc
// as a CRDT (`engine_sync::relay`), never bridged to the engine itself
// (#316).

/// The song `guid`'s one doc, as a side of a relay: what it is passed
/// reaches its engine through its bridge and is kept in its file. A song
/// open from something other than a project file (an `.RPP`) is begun on
/// its project file first — `Song.session` beside what
/// it was opened from (`opened_from`), as a save would make it.
///
/// # Errors
/// The song is not in `daw`, or its file would not open or take the
/// write.
pub fn live_side(
    daw: &Standalone,
    guid: &str,
    opened_from: &Path,
) -> Result<engine_sync::relay::Side, String> {
    // Every step read is in the doc first.
    written(WRITE_WAIT);
    let (daw, guid) = (daw.clone(), guid.to_owned());
    let (file, branch) = save_target(opened_from);
    run(async move {
        let mut songs = open_songs().lock().await;
        if !songs.contains_key(&guid) {
            let mut open = begin(&daw, &guid, &file, &branch).await?;
            append(&mut open, "Live")?;
            open._remote = Some(watch_remote(&guid, open.bridge.doc().loro()));
            forget_undo(&guid);
            songs.insert(guid.clone(), open);
        }
        let open = songs.get(&guid).ok_or("the song is not open")?;
        Ok(side_of(open))
    })?
}

/// `open`'s doc as a relay's side: an import, which the watch makes the
/// engine match and appends.
fn side_of(open: &Open) -> engine_sync::relay::Side {
    engine_sync::relay::Side::imported(open.bridge.doc().loro().clone())
}

/// Why a song cannot be moved onto a live set's doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotAdopted {
    /// The song is not open from a project file here.
    NotOpen,
    /// What went wrong on the way (the file, the engine).
    Failed(String),
}

impl std::fmt::Display for NotAdopted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotOpen => f.write_str("the song is not open from a project file"),
            Self::Failed(why) => f.write_str(why),
        }
    }
}

/// A song moved onto a live set's doc ([`adopt`]).
pub struct Adopted {
    /// The song's doc, now of the set's lineage, as a relay's side.
    pub side: engine_sync::relay::Side,
    /// The branch of the song's file its own version is kept as.
    pub kept_as: String,
}

/// The branch a song's own version is kept as when it moves onto a live
/// set's doc: `Before joining <set> (<date>)` — a name a branch can have
/// (no path separator in it).
#[must_use]
pub fn archive_name(set: &str, date: &str) -> String {
    let set = set.trim().replace(['/', '\\'], "-");
    let set = if set.is_empty() { "a live set" } else { &set };
    format!("Before joining {set} ({date})")
}

/// `unix_secs` as a calendar date (UTC), `2026-10-29`.
#[must_use]
pub fn date_of(unix_secs: u64) -> String {
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let days = i64::try_from(unix_secs / 86_400).unwrap_or(0) + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Move the song `guid` onto `live`, a live set's doc of it that another
/// peer seeded from its own copy (so not of this song's lineage, and not
/// to be merged with it). Nothing of the song's own version is lost: it is
/// kept in the song's file as the branch `archive` (made unique), listed
/// with its alternatives, and opens as one. Then its one bridge is
/// switched to a copy of `live` (the engine made to match it, nothing
/// built again that it holds), and its branch starts over from that copy.
///
/// # Errors
/// See [`NotAdopted`].
pub fn adopt(
    guid: &str,
    live: &engine_sync::loro::LoroDoc,
    archive: &str,
) -> Result<Adopted, NotAdopted> {
    // Every step read is in the doc first.
    written(WRITE_WAIT);
    let frames = engine_sync::relay::seed(live).map_err(|e| NotAdopted::Failed(e.to_string()))?;
    let (guid, archive) = (guid.to_owned(), archive.to_owned());
    run(async move {
        let mut songs = open_songs().lock().await;
        let open = songs.get_mut(&guid).ok_or(NotAdopted::NotOpen)?;
        let copy = engine_sync::loro::LoroDoc::new();
        copy.import_batch(&frames)
            .map_err(|e| NotAdopted::Failed(format!("the live doc would not copy: {e}")))?;
        // The song's own version, whole: what it holds unsaved written
        // first, then kept aside in its file.
        append(open, "Before joining").map_err(NotAdopted::Failed)?;
        let own = open.bridge.doc().loro().clone();
        let kept_as = with_store(&open.store, |s| {
            keep_aside(s, &open.branch, &own, &archive).map_err(|e| e.to_string())
        })
        .map_err(NotAdopted::Failed)?;
        let switched = open
            .bridge
            .switch_doc(SessionDoc::from_loro(copy))
            .await
            .map_err(|e| e.to_string());
        // Whatever the engine made of it, the bridge is on the copy now:
        // the file follows it, so what it appends is of that lineage.
        let doc = open.bridge.doc().loro().clone();
        with_store(&open.store, |s| {
            s.rebase(&open.branch, &doc)
                .and_then(|()| s.checkpoint())
                .map_err(|e| e.to_string())
        })
        .map_err(NotAdopted::Failed)?;
        open.saved = open.bridge.doc().version();
        open.uncompacted = 0;
        open._remote = Some(watch_remote(&guid, &doc));
        tracing::info!(
            project.guid = %guid,
            song.kept_as = %kept_as,
            "the song moved onto the live set's doc of it"
        );
        let side = side_of(open);
        drop(songs);
        switched.map_err(NotAdopted::Failed)?;
        Ok(Adopted { side, kept_as })
    })
    .map_err(NotAdopted::Failed)?
}

/// Keep `own`, `branch`'s version as it is, in `store` as a branch of its
/// own called `name` (or `name 2`, `name 3`, … when that is taken), before
/// `branch` starts over from another lineage. Returns the name it got.
///
/// Every other branch is made whole first (its own snapshot, no longer read
/// through the branch it was forked from): one forked from `branch` reads
/// `branch`'s history up to where it forked, which is about to go. The kept
/// branch is whole too, so it opens whatever `branch` becomes.
fn keep_aside(
    store: &Store,
    branch: &str,
    own: &engine_sync::loro::LoroDoc,
    name: &str,
) -> session_format::store::StoreResult<String> {
    let branches = store.branches()?;
    for other in branches.iter().filter(|b| b.name != branch) {
        let whole = store.load(&other.name)?;
        store.rebase(&other.name, &whole)?;
    }
    let taken = |n: &str| branches.iter().any(|b| b.name == n);
    let kept = std::iter::once(name.to_owned())
        .chain((2..).map(|n| format!("{name} {n}")))
        .find(|n| !taken(n))
        .unwrap_or_else(|| name.to_owned());
    store.fork(&kept, branch, own)?;
    store.rebase(&kept, own)?;
    // Its steps and marks go with it: their frontiers are of its lineage,
    // not the one `branch` starts over from.
    store.move_history(branch, &kept)?;
    Ok(kept)
}

/// Whether the song `guid` is open from its project file here — and so
/// has its one bridge.
#[must_use]
pub fn is_open(guid: &str) -> bool {
    run(async { open_songs().lock().await.contains_key(guid) }).unwrap_or(false)
}

mod undo_groups;

#[cfg(test)]
mod tests;
