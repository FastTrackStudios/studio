//! A session's file: one format for every app that holds a session.
//!
//! A session on disk is a document file ([`doc_file`]) of kind
//! [`crate::doc::KIND`]: its edits as steps, its id and kind in its
//! header. The DAW engine, Keyflow and any other host open and save it the
//! same way, here, so each can open the others' sessions: one session per
//! song, not one per app. The bytes are the ones Architect's document file
//! wrote, so a session or chart made before still opens.
//!
//! **No unsaved state.** Every commit to [`SessionFile::session`] is
//! appended as it is made, by a writer thread of the file's own (so an
//! edit never waits on the disk, and an edit made while the caller holds
//! the backing store's lock cannot deadlock on it). [`SessionFile::save`]
//! folds the steps into one snapshot.
//!
//! A session written before this as a bare Loro snapshot (what the DAW
//! engine wrote) is brought into the format once, by [`SessionFile::import`]
//! or [`SessionFile::open_or_import`]: same content, a document file from
//! then on.
//!
//! A song's session lives inside the song's one file (`<Title>.session`,
//! the DAW's project store, `docs/design/one-file.md`): there it is not a
//! document file of its own but a part of that store ([`PartStore`]), and
//! is opened with [`SessionFile::over_part`]. The session itself, its id
//! and every edit are the same either way.
//!
//! Which a file is, is said by its first bytes ([`sniff`]), never by its
//! name.

mod doc_file;
mod format;
mod index;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;

use loro::LoroDoc;
use uuid::Uuid;

use self::doc_file::DocFile;
use crate::doc::{KIND, SCHEMA, SessionDoc};

/// Why a session file could not be opened, made or saved.
#[derive(Debug)]
pub enum SessionFileError {
    /// The file: missing, not a document file, another kind, unwritable.
    File(String),
    /// The document refused what it was given.
    Doc(String),
}

impl std::fmt::Display for SessionFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::File(e) => write!(f, "session file: {e}"),
            Self::Doc(e) => write!(f, "session document: {e}"),
        }
    }
}

impl std::error::Error for SessionFileError {}

type Result<T> = std::result::Result<T, SessionFileError>;

/// What went wrong with a document file's bytes.
#[derive(Debug, thiserror::Error)]
enum FileError {
    #[error("fs: {0}")]
    Storage(String),
    #[error("not a document file: {0}")]
    NotADocument(String),
    #[error("loro: {0}")]
    Loro(String),
    #[error("this is a {found} file, not a {expected}")]
    WrongKind { found: String, expected: String },
    #[error("schema: {0}")]
    Schema(String),
    #[error("no branch {0}")]
    NoBranch(String),
}

#[allow(clippy::needless_pass_by_value)] // An adapter for `map_err`.
fn file_error(e: FileError) -> SessionFileError {
    SessionFileError::File(e.to_string())
}

/// What a song's file starts with, as [`sniff`] reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sniffed {
    /// A document file: a session file of its own (`<Title>.fts-session`,
    /// before the song's one file).
    DocFile,
    /// A SQLite (Turso) store: the song's one file, whose session is a
    /// part of it ([`SessionFile::over_part`]).
    Store,
    /// Neither.
    Other,
}

/// What the file starting with `head` (16 bytes are enough) is.
#[must_use]
pub fn sniff(head: &[u8]) -> Sniffed {
    if head.starts_with(b"ARCHDOC\0") {
        Sniffed::DocFile
    } else if head.starts_with(b"SQLite format 3\0") {
        Sniffed::Store
    } else {
        Sniffed::Other
    }
}

/// A session document file's id and each of its branches, read from the
/// file's bytes (it is not opened): what a converter reads an old session
/// file by. `main` comes first.
///
/// # Errors
///
/// Not a session's document file, or a branch that would not load.
pub fn read_branches(bytes: &[u8]) -> Result<(Uuid, Vec<(String, LoroDoc)>)> {
    doc_file::read_branches(bytes, KIND).map_err(file_error)
}

/// A file a session is a part of (the song's one file): it answers for
/// the session by its id. Called from the session file's writer thread,
/// never while the session's document is being committed to.
pub trait PartStore: Send + Sync + 'static {
    /// The session `id` as the file holds it, as a Loro snapshot; `None`
    /// before it holds any.
    ///
    /// # Errors
    ///
    /// The file would not read.
    fn load(&self, id: Uuid) -> std::result::Result<Option<Vec<u8>>, String>;

    /// Keeps one update to session `id` (a commit, or one that arrived).
    ///
    /// # Errors
    ///
    /// The file would not take it.
    fn append(&self, id: Uuid, update: &[u8]) -> std::result::Result<(), String>;

    /// Folds session `id` into `snapshot` (its whole state; what the file
    /// holds besides is kept too).
    ///
    /// # Errors
    ///
    /// The file would not take it.
    fn compact(&self, id: Uuid, snapshot: &[u8]) -> std::result::Result<(), String>;
}

/// Where a session's writes go: its own document file, or the file it is
/// a part of.
enum Backing {
    File(DocFile),
    Part(Uuid, Arc<dyn PartStore>),
}

impl Backing {
    fn append(&mut self, update: &[u8]) -> std::result::Result<(), String> {
        match self {
            Self::File(file) => file.append(update).map_err(|e| e.to_string()),
            Self::Part(id, part) => part.append(*id, update),
        }
    }

    fn fold(&mut self, snapshot: Vec<u8>) -> std::result::Result<(), String> {
        match self {
            Self::File(file) => file.fold(snapshot).map_err(|e| e.to_string()),
            Self::Part(id, part) => part.compact(*id, &snapshot),
        }
    }
}

enum Write {
    /// A commit, or an update that arrived from elsewhere ([`Keeper`]).
    Update(Vec<u8>),
    /// Fold everything into this snapshot, then answer.
    Fold(Vec<u8>, mpsc::Sender<std::result::Result<(), String>>),
}

/// Keeps updates that reach the session from elsewhere in its file.
///
/// A sync joiner's imports fire no commit, so they are handed here and
/// written like the session's own edits. It does not keep the file open:
/// once the session is let go, what it is handed is not written.
#[derive(Clone)]
pub struct Keeper(std::sync::Weak<mpsc::Sender<Write>>);

impl Keeper {
    /// Writes `update` to the session's file (after what is queued).
    pub fn keep(&self, update: Vec<u8>) {
        if let Some(tx) = self.0.upgrade() {
            let _ = tx.send(Write::Update(update));
        }
    }
}

/// The session's writer thread and what feeds it. Dropped, it finishes
/// writing what is queued before the file is let go.
struct Writer {
    /// The one strong handle on the queue's sender ([`Keeper`]s hold weak
    /// ones), besides the commit subscription's own.
    tx: Option<Arc<mpsc::Sender<Write>>>,
    thread: Option<std::thread::JoinHandle<()>>,
    commits: Option<loro::Subscription>,
}

impl Writer {
    fn start(doc: &LoroDoc, mut backing: Backing) -> Result<Self> {
        let (tx, rx) = mpsc::channel::<Write>();
        let thread = std::thread::Builder::new()
            .name("session-file".into())
            .spawn(move || {
                for write in rx {
                    match write {
                        Write::Update(update) => {
                            if let Err(e) = backing.append(&update) {
                                // The next save folds the whole document
                                // in, this update with it.
                                tracing::warn!(
                                    session_file.outcome = "append-failed",
                                    "session file: an edit was not written: {e}"
                                );
                            }
                        }
                        Write::Fold(snapshot, done) => {
                            let _ = done.send(backing.fold(snapshot));
                        }
                    }
                }
            })
            .map_err(|e| SessionFileError::File(format!("its writer: {e}")))?;
        let commit = tx.clone();
        let commits = doc.subscribe_local_update(Box::new(move |update| {
            commit.send(Write::Update(update.clone())).is_ok()
        }));
        Ok(Self {
            tx: Some(Arc::new(tx)),
            thread: Some(thread),
            commits: Some(commits),
        })
    }

    fn keeper(&self) -> Option<Keeper> {
        self.tx.as_ref().map(|tx| Keeper(Arc::downgrade(tx)))
    }

    fn fold(&self, snapshot: Vec<u8>) -> Result<()> {
        let tx = self
            .tx
            .as_ref()
            .ok_or_else(|| SessionFileError::File("closed".into()))?;
        let (done, answer) = mpsc::channel();
        tx.send(Write::Fold(snapshot, done))
            .map_err(|_| SessionFileError::File("its writer stopped".into()))?;
        answer
            .recv()
            .map_err(|_| SessionFileError::File("its writer stopped".into()))?
            .map_err(SessionFileError::File)
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        // The commit subscription holds a sender too: every one must go
        // for the thread to see the end of its queue.
        self.commits.take();
        self.tx.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A session held open in its file. Edits to [`Self::session`] are steps
/// of the file as they are committed; [`Self::save`] folds them.
pub struct SessionFile {
    id: Uuid,
    session: SessionDoc,
    writer: Writer,
}

impl SessionFile {
    /// Makes a new session file at `path`.
    ///
    /// # Errors
    ///
    /// The file could not be made.
    pub fn create(path: &Path) -> Result<Self> {
        let id = Uuid::new_v4();
        let file = DocFile::create(path, KIND, id, Some(SCHEMA)).map_err(file_error)?;
        let this = Self::over(id, LoroDoc::new(), Backing::File(file))?;
        // Stamped at this build's schema, not upgraded to it: a new
        // session has nothing to migrate. Its first step.
        crate::doc::stamp(this.session.doc());
        Ok(this)
    }

    /// Opens the session file at `path`.
    ///
    /// # Errors
    ///
    /// No such file, not a document file (see [`Self::open_or_import`]),
    /// another kind's, or from a newer build.
    pub fn open(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Err(SessionFileError::File(format!(
                "{}: no such file",
                path.display()
            )));
        }
        let file = DocFile::open(path, KIND, SCHEMA).map_err(file_error)?;
        let id = file
            .id()
            .ok_or_else(|| SessionFileError::File("the file names no document".into()))?;
        let doc = file.main().map_err(file_error)?;
        crate::doc::check_schema(&doc).map_err(SessionFileError::File)?;
        Self::over(id, doc, Backing::File(file))
    }

    /// A session file at `path` for session `id` made on another device:
    /// empty until it syncs, of the same lineage (nothing is written into
    /// it, not even its stamp, which arrives with its history).
    ///
    /// # Errors
    ///
    /// The file could not be made.
    pub fn adopt(path: &Path, id: Uuid) -> Result<Self> {
        let file = DocFile::create(path, KIND, id, None).map_err(file_error)?;
        Self::over(id, LoroDoc::new(), Backing::File(file))
    }

    /// A new session file at `path` holding the session in `snapshot`, a
    /// bare Loro snapshot (a session saved before the file format).
    ///
    /// # Errors
    ///
    /// The file could not be made, or `snapshot` is not a Loro document.
    pub fn import(path: &Path, snapshot: &[u8]) -> Result<Self> {
        let this = Self::create(path)?;
        this.session
            .doc()
            .import(snapshot)
            .map_err(|e| SessionFileError::Doc(e.to_string()))?;
        this.session.doc().commit();
        this.save()?;
        Ok(this)
    }

    /// Opens the session at `path`: a session file as it is, or a bare
    /// Loro snapshot brought into the format in place (the snapshot kept
    /// beside it as `<path>.snapshot`), so an app opens a session another
    /// app saved either way.
    ///
    /// # Errors
    ///
    /// No such file, or neither a session file nor a Loro document.
    pub fn open_or_import(path: &Path) -> Result<Self> {
        match Self::open(path) {
            Err(SessionFileError::File(why)) if why.starts_with("not a document file") => {
                let snapshot = std::fs::read(path)
                    .map_err(|e| SessionFileError::File(format!("{}: {e}", path.display())))?;
                // Keep what was there; the session file takes its place.
                let mut kept = PathBuf::from(path).into_os_string();
                kept.push(".snapshot");
                std::fs::rename(path, &kept)
                    .map_err(|e| SessionFileError::File(format!("{}: {e}", path.display())))?;
                Self::import(path, &snapshot)
            }
            other => other,
        }
    }

    /// The session `id` kept in `part`: a part of another file (the song's
    /// one file, whose store answers for its parts by id). What it holds
    /// is loaded; every commit is appended there, and [`Self::save`] folds
    /// it there.
    ///
    /// # Errors
    ///
    /// What the file holds would not load.
    pub fn over_part(id: Uuid, part: Arc<dyn PartStore>) -> Result<Self> {
        let doc = LoroDoc::new();
        if let Some(snapshot) = part.load(id).map_err(SessionFileError::File)? {
            doc.import(&snapshot)
                .map_err(|e| SessionFileError::Doc(e.to_string()))?;
        }
        Self::over(id, doc, Backing::Part(id, part))
    }

    fn over(id: Uuid, doc: LoroDoc, backing: Backing) -> Result<Self> {
        let writer = Writer::start(&doc, backing)?;
        Ok(Self {
            id,
            session: SessionDoc::from_doc(doc),
            writer,
        })
    }

    /// The session's document id: what a pane, a share link and the sync
    /// driver name it by.
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    /// The session.
    #[must_use]
    pub const fn session(&self) -> &SessionDoc {
        &self.session
    }

    /// What keeps updates that arrive from elsewhere (a sync joiner's
    /// imports) in this file, as its own commits are.
    ///
    /// # Errors
    ///
    /// The file is closed.
    pub fn keeper(&self) -> Result<Keeper> {
        self.writer
            .keeper()
            .ok_or_else(|| SessionFileError::File("closed".into()))
    }

    /// Writes everything the session holds to its file and folds the
    /// file's records into one: its whole state, so an update that
    /// arrived from elsewhere and was not kept is in it too.
    ///
    /// # Errors
    ///
    /// The file could not be written.
    pub fn save(&self) -> Result<()> {
        let state = self
            .session
            .doc()
            .export(loro::ExportMode::Snapshot)
            .map_err(|e| SessionFileError::Doc(e.to_string()))?;
        self.writer.fold(state)
    }
}
