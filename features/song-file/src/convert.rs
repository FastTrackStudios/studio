//! Folding the old pair into one file.
//!
//! A folder converted from
//!
//! ```text
//! Drive To LA.session          the engine's store (+ -wal, -tshm)
//! Drive To LA.fts-session      the session's Architect document
//! ```
//!
//! to `Drive To LA.session`, the whole song. The steps, each safe to stop
//! after:
//!
//! 1. **A new file.** The engine's store is copied (with its log) to
//!    `<Title>.session.new` and opened there; the session document's every
//!    branch goes in as the store's `session` part, history and all; the
//!    song takes the document's id. The old files are only read.
//! 2. **Verified.** The new file's bytes alone (no log beside them) are
//!    read back: the id, each branch's session (every value and the whole
//!    history: the same version vector), and the engine's project (each
//!    branch's doc, the blobs, the history) against a copy of the old
//!    store. Any difference stops here, the old pair untouched.
//! 3. **The old pair to the Trash** (the desktop's, never deleted), then
//!    the new file renamed to `<Title>.session`. Its folder's proxies are
//!    copied into the new layout ([`crate::media`]).
//!
//! Run again on a converted folder it does nothing; on a folder stopped
//! between steps it verifies what is there and goes on.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crdt::DocFile;
use crdt::persistence::Persistence as _;
use fts_session::doc::SessionDoc;
use loro::LoroDoc;
use session_format::read::ProjectFile;
use session_format::store::{SESSION, Store};
use uuid::Uuid;

use crate::{Error, Layout, Result, file_error};

/// How to convert.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Build and verify in a temporary folder, and change nothing in the
    /// song's folder.
    pub dry_run: bool,
    /// Move the old pair into this folder instead of the desktop's Trash
    /// (tests; or a person who keeps them somewhere of their own).
    pub trash_dir: Option<PathBuf>,
}

/// Put `files` out of the way: the desktop's Trash, or `dir`.
fn put_away(files: &[PathBuf], dir: Option<&Path>) -> Result<()> {
    match dir {
        None => trash::delete_all(files).map_err(|e| Error::Refused(format!("the Trash: {e}"))),
        Some(dir) => {
            std::fs::create_dir_all(dir)?;
            for f in files {
                let Some(name) = f.file_name() else { continue };
                let to = dir.join(name);
                if to.exists() {
                    return Err(Error::Refused(format!(
                        "{} is there already: not overwritten",
                        to.display()
                    )));
                }
                std::fs::rename(f, &to)?;
            }
            Ok(())
        }
    }
}

/// What converting a folder did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The folder holds no song.
    Nothing,
    /// The folder is one file already.
    AlreadyOneFile { file: PathBuf, id: Option<String> },
    /// Converted (or, dry, would be).
    Converted(Report),
}

/// What a conversion found and wrote.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub title: String,
    pub id: String,
    /// The one file (in a dry run, where it was built).
    pub file: PathBuf,
    /// The session's branches, each with what it holds.
    pub sessions: BTreeMap<String, SessionSummary>,
    /// The engine's branches.
    pub project_branches: Vec<String>,
    /// The blobs the engine's project keeps.
    pub blobs: usize,
    /// What went to the Trash.
    pub trashed: Vec<PathBuf>,
    /// Proxies copied into the new layout: (from, to).
    pub proxies: Vec<(PathBuf, PathBuf)>,
}

/// What one branch of a session holds, as the report shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionSummary {
    pub tempo_points: usize,
    pub markers: usize,
    pub regions: usize,
    pub lanes: usize,
    pub items: usize,
    pub bar_one: bool,
    /// Each extension's entries, by name.
    pub extensions: BTreeMap<String, Vec<String>>,
}

impl SessionSummary {
    fn of(doc: &LoroDoc) -> Self {
        let session = SessionDoc::from_doc(doc.clone()).read();
        Self {
            tempo_points: session.tempo.points().len(),
            markers: session.markers.len(),
            regions: session.regions.len(),
            lanes: session.lanes.len(),
            items: session.items.len(),
            bar_one: session.bar_one.is_some(),
            extensions: session
                .extension_ids()
                .map(|id| {
                    let entries = session
                        .ext(id)
                        .map(|d| d.entries.keys().cloned().collect())
                        .unwrap_or_default();
                    (id.as_str().to_owned(), entries)
                })
                .collect(),
        }
    }
}

/// The old session document: its id and each branch's doc, read from its
/// bytes (the file itself is never opened for writing).
struct OldSession {
    id: Uuid,
    branches: BTreeMap<String, LoroDoc>,
}

async fn read_old_session(path: &Path) -> Result<OldSession> {
    let bytes = std::fs::read(path)?;
    let file = DocFile::from_bytes(bytes, &fts_session::doc::kind())
        .await
        .map_err(|e| file_error(path, e))?;
    let id = file
        .id()
        .await
        .ok_or_else(|| file_error(path, "the document names no id"))?;
    let mut branches = BTreeMap::new();
    for branch in file.branches().await {
        // As the session opens it: its snapshot, then its steps.
        let persistence = file.persistence(&branch.name);
        let doc = LoroDoc::new();
        if let Some(snapshot) = persistence
            .load_snapshot(id)
            .await
            .map_err(|e| file_error(path, e))?
        {
            doc.import(&snapshot).map_err(|e| file_error(path, e))?;
        }
        for update in persistence
            .load_updates(id)
            .await
            .map_err(|e| file_error(path, e))?
        {
            doc.import(&update).map_err(|e| file_error(path, e))?;
        }
        branches.insert(branch.name, doc);
    }
    Ok(OldSession { id, branches })
}

/// A store's files: the file, its log and its shared memory.
fn store_files(path: &Path) -> [PathBuf; 3] {
    let with = |suffix: &str| {
        let mut p = path.as_os_str().to_owned();
        p.push(suffix);
        PathBuf::from(p)
    };
    [path.to_path_buf(), with("-wal"), with("-tshm")]
}

/// Copy a store with its log to `to` (never its shared memory: the copy
/// is opened by one process, which makes its own).
fn copy_store(from: &Path, to: &Path) -> Result<()> {
    let [file, wal, _] = store_files(from);
    let [to_file, to_wal, _] = store_files(to);
    for (a, b) in [(&file, &to_file), (&wal, &to_wal)] {
        if a.is_file() {
            if b.exists() {
                return Err(Error::Refused(format!(
                    "{} is there already: not overwritten",
                    b.display()
                )));
            }
            std::fs::copy(a, b)?;
        }
    }
    Ok(())
}

/// Write the one file at `new`: the engine's store copied from `project`
/// (when there is one), the session's branches as its parts, its id the
/// session's.
fn build(new: &Path, project: Option<&Path>, old: &OldSession) -> Result<()> {
    let _span = tracing::info_span!("song_file.build").entered();
    if let Some(project) = project {
        copy_store(project, new)?;
    } else if new.exists() {
        return Err(Error::Refused(format!(
            "{} is there already: not overwritten",
            new.display()
        )));
    }
    let store = Store::open(new).map_err(|e| file_error(new, e))?;
    store.set_song_id(old.id)?;
    for (branch, doc) in &old.branches {
        store.compact_part(SESSION, branch, doc)?;
    }
    store.close()?;
    Ok(())
}

/// Each branch's doc, the blob names and how many moments of history: an
/// engine's project as compared.
type ProjectShape = (
    BTreeMap<String, loro::LoroValue>,
    Vec<String>,
    Vec<(String, u32)>,
);

fn project_shape(store: &Store) -> Result<ProjectShape> {
    let mut docs = BTreeMap::new();
    let mut history = Vec::new();
    for b in store.branches()? {
        docs.insert(b.name.clone(), store.load(&b.name)?.get_deep_value());
        history.push((b.name.clone(), store.history(&b.name, (0, 0))?.1));
    }
    Ok((docs, blob_names(store)?, history))
}

fn blob_names(store: &Store) -> Result<Vec<String>> {
    let mut names = store.blob_names()?;
    names.sort();
    Ok(names)
}

/// Check the one file at `new` holds `old` and `project`; what it holds.
fn verify(new: &Path, project: Option<&Path>, old: &OldSession) -> Result<Report> {
    let _span = tracing::info_span!("song_file.verify").entered();
    // The file's bytes alone, as a drive or a browser would have them.
    let bytes = std::fs::read(new)?;
    let read = ProjectFile::read(&bytes).map_err(|e| Error::Mismatch(e.to_string()))?;
    let id = old.id.to_string();
    if read.song_id() != Some(id.as_str()) {
        return Err(Error::Mismatch(format!(
            "the id is {:?}, the session's {id}",
            read.song_id()
        )));
    }
    let mut sessions = BTreeMap::new();
    for (branch, doc) in &old.branches {
        let got = read
            .part(SESSION, branch)
            .map_err(|e| Error::Mismatch(e.to_string()))?;
        if got.get_deep_value() != doc.get_deep_value() {
            return Err(Error::Mismatch(format!("the session's {branch} differs")));
        }
        if got.oplog_vv() != doc.oplog_vv() {
            return Err(Error::Mismatch(format!(
                "the session's {branch} lost history"
            )));
        }
        sessions.insert(branch.clone(), crate::convert::SessionSummary::of(&got));
    }

    // The engine's project: the new file copied alone, opened as a store,
    // against a copy of the old store (with its log) — never the old files
    // themselves, which opening would write to.
    let scratch = tempfile::tempdir()?;
    let alone = scratch.path().join("new.session");
    std::fs::copy(new, &alone)?;
    let got = Store::open(&alone).map_err(|e| file_error(new, e))?;
    if got.song_id()?.map(|u| u.to_string()) != Some(id.clone()) {
        return Err(Error::Mismatch("the store says another id".into()));
    }
    let got_shape = project_shape(&got)?;
    if let Some(project) = project {
        let was = scratch.path().join("old.session");
        copy_store(project, &was)?;
        let want = Store::open(&was).map_err(|e| file_error(project, e))?;
        let want_shape = project_shape(&want)?;
        if got_shape.0 != want_shape.0 {
            return Err(Error::Mismatch("the engine's project differs".into()));
        }
        if got_shape.1 != want_shape.1 {
            return Err(Error::Mismatch("the engine's blobs differ".into()));
        }
        if got_shape.2 != want_shape.2 {
            return Err(Error::Mismatch("the engine's history differs".into()));
        }
    }
    Ok(Report {
        id,
        file: new.to_path_buf(),
        sessions,
        project_branches: got_shape.0.keys().cloned().collect(),
        blobs: got_shape.1.len(),
        ..Report::default()
    })
}

/// `<Title>.session.new`: where the one file is built beside the pair.
fn pending(target: &Path) -> PathBuf {
    let mut p = target.as_os_str().to_owned();
    p.push(".new");
    PathBuf::from(p)
}

/// Fold `folder`'s old pair into `<Title>.session`.
///
/// # Errors
///
/// The folder holds more than one song, a file would not read, the new
/// file does not verify (the old pair is then untouched), or the Trash
/// refused.
pub async fn convert(folder: &Path, options: &Options) -> Result<Outcome> {
    use tracing::Instrument as _;
    let span = tracing::info_span!(
        "song_file.convert",
        song.dry_run = options.dry_run,
        song.outcome = tracing::field::Empty,
        song.id = tracing::field::Empty,
        song.sessions = tracing::field::Empty,
        song.trashed = tracing::field::Empty,
    );
    convert_in(folder, options).instrument(span).await
}

async fn convert_in(folder: &Path, options: &Options) -> Result<Outcome> {
    let span = tracing::Span::current();
    let (project, session) = match Layout::of(folder)? {
        Layout::Empty => {
            span.record("song.outcome", "nothing");
            return Ok(Outcome::Nothing);
        }
        Layout::OneFile { file } => {
            span.record("song.outcome", "already");
            // Opened to read its id, and closed again: its log folded in.
            let id = Store::open(&file).ok().and_then(|s| {
                let id = s.song_id().ok().flatten();
                s.close().ok()?;
                id.map(|u| u.to_string())
            });
            return Ok(Outcome::AlreadyOneFile { file, id });
        }
        Layout::Pair { project, session } => (project, session),
    };
    let title = session
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .ok_or_else(|| file_error(&session, "no name"))?;
    let target = folder.join(format!("{title}.{}", crate::EXTENSION));
    if project.is_none() && target.exists() {
        return Err(file_error(
            &target,
            "is there but is not a store: convert it by hand",
        ));
    }
    let old = read_old_session(&session).await?;

    let scratch = tempfile::tempdir()?;
    let new = if options.dry_run {
        scratch.path().join(format!("{title}.session"))
    } else {
        pending(&target)
    };
    // A run stopped after building finds its file and verifies it.
    if !new.exists() {
        build(&new, project.as_deref(), &old)?;
    }
    let mut report = verify(&new, project.as_deref(), &old)?;
    report.title = title.clone();
    if options.dry_run {
        span.record("song.outcome", "dry");
        report.proxies = crate::media::plan(folder)?;
        return Ok(Outcome::Converted(report));
    }

    // The old pair to the Trash: the session document, the engine's store
    // and its log, and the new file's own log (empty once closed).
    let mut old_files: Vec<PathBuf> = vec![session.clone()];
    if let Some(project) = &project {
        old_files.extend(store_files(project).into_iter().filter(|p| p.exists()));
    }
    put_away(&old_files, options.trash_dir.as_deref())?;
    report.trashed = old_files;
    if target.exists() {
        return Err(file_error(
            &target,
            "is still there after the Trash: the new file stays as .session.new",
        ));
    }
    std::fs::rename(&new, &target)?;
    let [_, new_wal, new_shm] = store_files(&new);
    let leftovers: Vec<PathBuf> = [new_wal, new_shm]
        .into_iter()
        .filter(|p| p.exists())
        .collect();
    if !leftovers.is_empty() {
        put_away(&leftovers, options.trash_dir.as_deref())?;
        report.trashed.extend(leftovers);
    }
    report.file = target;
    report.proxies = crate::media::copy_into_layout(folder)?;
    span.record("song.outcome", "converted");
    span.record("song.id", report.id.as_str());
    span.record("song.sessions", report.sessions.len());
    span.record("song.trashed", report.trashed.len());
    Ok(Outcome::Converted(report))
}
