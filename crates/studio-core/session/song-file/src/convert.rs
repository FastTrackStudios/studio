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
#[cfg(not(target_os = "ios"))]
fn put_away(files: &[PathBuf], dir: Option<&Path>) -> Result<()> {
    match dir {
        None => trash::delete_all(files).map_err(|e| Error::Refused(format!("the Trash: {e}"))),
        Some(dir) => move_into(files, dir),
    }
}

/// Put `files` out of the way: on iPhone and iPad, which have no Trash an
/// app can use, `Replaced` in the song's own folder (where the Files app
/// shows them), or `dir`. Never deleted.
#[cfg(target_os = "ios")]
fn put_away(files: &[PathBuf], dir: Option<&Path>) -> Result<()> {
    let beside = files
        .first()
        .and_then(|f| f.parent())
        .map(|p| p.join("Replaced"));
    match dir.map(Path::to_path_buf).or(beside) {
        Some(dir) => move_into(files, &dir),
        None => Ok(()),
    }
}

/// Moves `files` into `dir`, overwriting nothing.
fn move_into(files: &[PathBuf], dir: &Path) -> Result<()> {
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

/// What converting a folder did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The folder holds no song.
    Nothing,
    /// The folder is one file already.
    AlreadyOneFile { file: PathBuf, id: Option<String> },
    /// Converted (or, dry, would be).
    Converted(Report),
    /// A store with no id (the engine's project alone): given an id and a
    /// session seeded from its own timing (or, dry, would be).
    Seeded(Report),
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
    /// What `doc` (a branch of the session part) holds.
    #[must_use]
    pub fn of(doc: &LoroDoc) -> Self {
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

fn read_old_session(path: &Path) -> Result<OldSession> {
    let bytes = std::fs::read(path)?;
    // As the session opens it: each branch's snapshot, then its steps.
    let (id, branches) =
        fts_session::file::read_branches(&bytes).map_err(|e| file_error(path, e))?;
    Ok(OldSession {
        id,
        branches: branches.into_iter().collect(),
    })
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
pub fn convert(folder: &Path, options: &Options) -> Result<Outcome> {
    let span = tracing::info_span!(
        "song_file.convert",
        song.dry_run = options.dry_run,
        song.outcome = tracing::field::Empty,
        song.id = tracing::field::Empty,
        song.sessions = tracing::field::Empty,
        song.trashed = tracing::field::Empty,
    );
    span.in_scope(|| convert_in(folder, options))
}

fn convert_in(folder: &Path, options: &Options) -> Result<Outcome> {
    let span = tracing::Span::current();
    let (project, session) = match Layout::of(folder)? {
        Layout::Empty => {
            span.record("song.outcome", "nothing");
            return Ok(Outcome::Nothing);
        }
        Layout::OneFile { file } => {
            // Read without opening the original (opening a store writes).
            if let Some(id) = read_id(&file)? {
                span.record("song.outcome", "already");
                return Ok(Outcome::AlreadyOneFile { file, id: Some(id) });
            }
            // A store with no id: the engine's project alone, no session
            // document ever made. It gets an id and a session from its own
            // timing.
            let report = seed(folder, &file, options)?;
            span.record(
                "song.outcome",
                if options.dry_run { "dry" } else { "seeded" },
            );
            span.record("song.id", report.id.as_str());
            return Ok(Outcome::Seeded(report));
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
    let old = read_old_session(&session)?;

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

/// The song id a store holds, read without opening it: from its bytes when
/// its log is empty, else from a scratch copy (store and log) opened as a
/// store.
fn read_id(file: &Path) -> Result<Option<String>> {
    let [_, wal, _] = store_files(file);
    let folded = std::fs::metadata(&wal).map_or(true, |m| m.len() == 0);
    if folded {
        return Ok(session_format::read::song_id(&std::fs::read(file)?));
    }
    let scratch = tempfile::tempdir()?;
    let copy = scratch.path().join("copy.session");
    copy_store(file, &copy)?;
    let store = Store::open(&copy).map_err(|e| file_error(file, e))?;
    Ok(store.song_id()?.map(|u| u.to_string()))
}

/// The session the engine's project `doc` says: its tempo changes with
/// their meters, its markers, its sections as regions, and bar 1 (a
/// `SONGSTART` marker, else the first section that is not an intro or a
/// count-in) — what the daw extension writes into a song's session when it
/// opens it.
pub fn session_from_project(doc: &LoroDoc) -> Result<LoroDoc> {
    use fts_session::time::{Position, TempoPoint, TimePosition, TimeSignature};
    use fts_session::{MarkId, Marker, Region, TempoMap, Time};

    let model = session_sync::SessionDoc::from_loro(doc.clone()).read();
    let out = SessionDoc::new();
    let err = |e: &dyn std::fmt::Display| Error::Session(e.to_string());
    let mut changes = model.tempo.clone();
    changes.sort_by(|a, b| a.at.total_cmp(&b.at));
    let meter = |c: &session_sync::model::TempoPoint| {
        TimeSignature::new(c.beats_per_bar.max(1), c.beat_unit.max(1))
    };
    let tempo = TempoMap::new(
        changes.first().map_or(120.0, |c| c.bpm),
        changes
            .first()
            .map_or_else(|| TimeSignature::new(4, 4), |c| meter(c)),
        changes
            .iter()
            .map(|c| {
                TempoPoint::with_time_signature(
                    Position::from_time(TimePosition::from_seconds(c.at)),
                    c.bpm,
                    meter(c),
                )
            })
            .collect(),
    );
    out.set_tempo(&tempo).map_err(|e| err(&e))?;
    for (id, m) in &model.markers {
        out.put_marker(
            &MarkId(format!("marker-{id}")),
            &Marker {
                at: Time::Seconds(m.at),
                name: m.name.clone(),
                color: m.color.map(|c| c & 0x00FF_FFFF),
                ruler: m.lane,
            },
        )
        .map_err(|e| err(&e))?;
    }
    for (id, r) in &model.regions {
        out.put_region(
            &MarkId(format!("region-{id}")),
            &Region {
                start: Time::Seconds(r.start),
                end: Time::Seconds(r.end),
                name: r.name.clone(),
                color: r.color.map(|c| c & 0x00FF_FFFF),
                ruler: r.lane,
            },
        )
        .map_err(|e| err(&e))?;
    }
    let songstart = model
        .markers
        .values()
        .find(|m| m.name.trim().eq_ignore_ascii_case("songstart"))
        .map(|m| m.at);
    let lead_in = |name: &str| {
        let n = name.trim().to_ascii_lowercase();
        n.starts_with("intro") || n.starts_with("count") || n.starts_with("click")
    };
    let mut sections: Vec<_> = model.regions.values().collect();
    sections.sort_by(|a, b| a.start.total_cmp(&b.start));
    let from_sections = match sections.first() {
        Some(first) if lead_in(&first.name) => {
            sections.iter().find(|s| !lead_in(&s.name)).map(|s| s.start)
        }
        _ => None,
    };
    out.set_bar_one(songstart.or(from_sections).map(Time::Seconds))
        .map_err(|e| err(&e))?;
    out.doc().commit();
    Ok(out.doc().clone())
}

/// Give a store with no id an id and a session seeded from its own timing:
/// in a new file (a copy of the store, the original only read), verified,
/// then — not dry — the original to the Trash and the new file renamed.
fn seed(folder: &Path, file: &Path, options: &Options) -> Result<Report> {
    let _span = tracing::info_span!("song_file.seed").entered();
    let title = file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .ok_or_else(|| file_error(file, "no name"))?;
    let scratch = tempfile::tempdir()?;
    let new = if options.dry_run {
        scratch.path().join(format!("{title}.session"))
    } else {
        pending(file)
    };
    // The project as the original holds it, from a scratch copy.
    let was = scratch.path().join("old.session");
    copy_store(file, &was)?;
    let want = Store::open(&was).map_err(|e| file_error(file, e))?;
    let want_shape = project_shape(&want)?;
    let seeded = session_from_project(&want.load(session_format::store::MAIN)?)?;

    // A run stopped after building finds its file and verifies it.
    if !new.exists() {
        copy_store(file, &new)?;
        let store = Store::open(&new).map_err(|e| file_error(&new, e))?;
        store.ensure_song_id()?;
        store.compact_part(SESSION, session_format::store::MAIN, &seeded)?;
        store.close()?;
    }

    // Verified from the new file's bytes alone.
    let bytes = std::fs::read(&new)?;
    let read = ProjectFile::read(&bytes).map_err(|e| Error::Mismatch(e.to_string()))?;
    let id = read
        .song_id()
        .ok_or_else(|| Error::Mismatch("the new file has no id".into()))?
        .to_owned();
    let part = read
        .part(SESSION, session_format::store::MAIN)
        .map_err(|e| Error::Mismatch(e.to_string()))?;
    if part.get_deep_value() != seeded.get_deep_value() {
        return Err(Error::Mismatch("the seeded session differs".into()));
    }
    let alone = scratch.path().join("new.session");
    std::fs::copy(&new, &alone)?;
    let got = Store::open(&alone).map_err(|e| file_error(&new, e))?;
    let got_shape = project_shape(&got)?;
    if got_shape != want_shape {
        return Err(Error::Mismatch("the engine's project differs".into()));
    }
    let mut report = Report {
        title,
        id,
        file: new.clone(),
        sessions: BTreeMap::from([(
            session_format::store::MAIN.to_owned(),
            SessionSummary::of(&part),
        )]),
        project_branches: got_shape.0.keys().cloned().collect(),
        blobs: got_shape.1.len(),
        ..Report::default()
    };
    if options.dry_run {
        report.proxies = crate::media::plan(folder)?;
        return Ok(report);
    }
    let old_files: Vec<PathBuf> = store_files(file)
        .into_iter()
        .filter(|p| p.exists())
        .collect();
    put_away(&old_files, options.trash_dir.as_deref())?;
    report.trashed = old_files;
    if file.exists() {
        return Err(file_error(
            file,
            "is still there after the Trash: the new file stays as .session.new",
        ));
    }
    std::fs::rename(&new, file)?;
    let [_, new_wal, new_shm] = store_files(&new);
    let leftovers: Vec<PathBuf> = [new_wal, new_shm]
        .into_iter()
        .filter(|p| p.exists())
        .collect();
    if !leftovers.is_empty() {
        put_away(&leftovers, options.trash_dir.as_deref())?;
        report.trashed.extend(leftovers);
    }
    report.file = file.to_path_buf();
    report.proxies = crate::media::copy_into_layout(folder)?;
    Ok(report)
}
