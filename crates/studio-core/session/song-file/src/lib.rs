//! A song as one file: `<Title>.session`.
//!
//! One Turso store (`session_format::store`) holds the whole song:
//!
//! - `meta.id`: the song's id, what a setlist names it by;
//! - the engine's project, per branch, as it always has;
//! - the session primitive's document (timing, sections, markers, and
//!   every extension's data: Keyflow's chart, Ignition's cues, Signal's
//!   patch changes) as the store's `session` part, per branch;
//! - arrangements and variations as branches (`<id>@<branch>`).
//!
//! [`SongFile`] opens it: the store, and the session over the store's
//! parts (`session_format::store::PartReplica`), so every edit of the
//! session is a row of the one file. Closing it folds the write-ahead log
//! in, so the file alone is the song, to copy or carry on a drive.
//!
//! A folder not converted yet still holds the old pair (`<Title>.session`
//! the engine's store, `<Title>.fts-session` the session's own Architect
//! document): [`Layout::of`] says which a folder holds, by the files'
//! headers, and [`convert`] folds the pair into one file.
//!
//! The design: the session repo's `docs/design/one-file.md`.

pub mod convert;
pub mod media;
pub mod new;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use fts_session::file::SessionFile;
use session_format::store::{MAIN, PartReplica, SESSION, Store};
use session_format::{FileFormat, sniff_path};
use uuid::Uuid;

pub use convert::{Options, Outcome, Report, SessionSummary, convert};
pub use new::{Made, Tempo, new_song};

/// The extension of a song's file.
pub const EXTENSION: &str = "session";
/// The extension of the old pair's session document.
pub const OLD_SESSION_EXTENSION: &str = "fts-session";

/// What went wrong with a song's file.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {why}")]
    File { path: PathBuf, why: String },
    #[error("the song's store: {0}")]
    Store(#[from] session_format::store::StoreError),
    #[error("the session: {0}")]
    Session(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("the converted file does not match the old pair: {0}")]
    Mismatch(String),
    #[error("{0}")]
    Refused(String),
}

pub type Result<T> = std::result::Result<T, Error>;

fn file_error(path: &Path, why: impl std::fmt::Display) -> Error {
    Error::File {
        path: path.to_path_buf(),
        why: why.to_string(),
    }
}

/// A song's file open: its store, and its session over the store.
pub struct SongFile {
    path: PathBuf,
    branch: String,
    store: Arc<Mutex<Store>>,
    session: SessionFile,
}

impl SongFile {
    /// Opens the song at `path` (a one-file `.session`, made if it is not
    /// there) on its `main` branch.
    ///
    /// # Errors
    ///
    /// The file is not a store, or would not open.
    pub async fn open(path: &Path) -> Result<Self> {
        Self::open_branch(path, MAIN).await
    }

    /// Opens the song at `path` on `branch` (an arrangement): the
    /// branch's own session.
    ///
    /// # Errors
    ///
    /// The file is not a store, or would not open.
    pub async fn open_branch(path: &Path, branch: &str) -> Result<Self> {
        use tracing::Instrument as _;
        let span = tracing::info_span!(
            "song_file.open",
            song.branch = branch,
            song.id = tracing::field::Empty,
        );
        Self::open_in(path, branch).instrument(span).await
    }

    async fn open_in(path: &Path, branch: &str) -> Result<Self> {
        if path.exists() && sniff_path(path) != FileFormat::Store {
            return Err(file_error(path, "not a song's one file (no store header)"));
        }
        let store = Store::open(path).map_err(|e| file_error(path, e))?;
        store.ensure_song_id()?;
        let id = store.part_id(SESSION, branch)?;
        tracing::Span::current().record("song.id", tracing::field::display(id));
        let store = Arc::new(Mutex::new(store));
        let session = SessionFile::over_persistence(id, Arc::new(PartReplica::new(store.clone())))
            .await
            .map_err(|e| Error::Session(e.to_string()))?;
        Ok(Self {
            path: path.to_path_buf(),
            branch: branch.to_owned(),
            store,
            session,
        })
    }

    /// The song's id: what a setlist names it by.
    ///
    /// # Errors
    ///
    /// The store refused the read.
    pub fn id(&self) -> Result<Uuid> {
        Ok(self.with_store(Store::ensure_song_id)?)
    }

    /// Where the song is.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The branch open.
    #[must_use]
    pub fn branch(&self) -> &str {
        &self.branch
    }

    /// The session: timing, sections, markers, every extension's data.
    #[must_use]
    pub const fn session(&self) -> &SessionFile {
        &self.session
    }

    /// The store, for the rest of what is done with the file.
    pub fn with_store<T>(&self, f: impl FnOnce(&Store) -> T) -> T {
        let store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
        f(&store)
    }

    /// Folds the session's edits into the file and its log into the file.
    ///
    /// # Errors
    ///
    /// The file could not be written.
    pub async fn save(&self) -> Result<()> {
        self.session
            .save()
            .await
            .map_err(|e| Error::Session(e.to_string()))?;
        self.with_store(Store::checkpoint)?;
        Ok(())
    }

    /// Saves and lets go of the file, its log folded in: the one file is
    /// the whole song.
    ///
    /// # Errors
    ///
    /// The file could not be written.
    pub async fn close(self) -> Result<()> {
        self.save().await?;
        let Self { store, session, .. } = self;
        drop(session);
        match Arc::try_unwrap(store) {
            Ok(store) => store
                .into_inner()
                .unwrap_or_else(PoisonError::into_inner)
                .close()?,
            // The session's writer still holds it a moment: fold the log
            // now; the store closes when the writer lets go.
            Err(shared) => shared
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .checkpoint()?,
        }
        Ok(())
    }
}

/// What a song's folder holds, by its files' headers (never their names).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Layout {
    /// `<Title>.session`, the whole song.
    OneFile { file: PathBuf },
    /// The old pair: the engine's store (when there is one) and the
    /// session's own document.
    Pair {
        project: Option<PathBuf>,
        session: PathBuf,
    },
    /// No song's file.
    Empty,
}

impl Layout {
    /// What `folder` holds.
    ///
    /// # Errors
    ///
    /// The folder would not list, or holds more than one song.
    pub fn of(folder: &Path) -> Result<Self> {
        let mut docs = Vec::new();
        let mut stores = Vec::new();
        for entry in std::fs::read_dir(folder)? {
            let path = entry?.path();
            if !path.is_file() {
                continue;
            }
            match sniff_path(&path) {
                FileFormat::DocFile => docs.push(path),
                FileFormat::Store if path.extension().is_some_and(|e| e == EXTENSION) => {
                    stores.push(path);
                }
                _ => {}
            }
        }
        docs.sort();
        stores.sort();
        match (docs.as_slice(), stores.as_slice()) {
            ([], []) => Ok(Self::Empty),
            ([], [one]) => Ok(Self::OneFile { file: one.clone() }),
            ([session], _) => {
                let project = session.with_extension(EXTENSION);
                Ok(Self::Pair {
                    project: stores.contains(&project).then_some(project),
                    session: session.clone(),
                })
            }
            _ => Err(file_error(
                folder,
                format!(
                    "{} session documents and {} stores: one song per folder",
                    docs.len(),
                    stores.len()
                ),
            )),
        }
    }
}

/// A song's id from its file's bytes (the start of the file is not enough
/// for a store: the whole file, folded), whichever format it is.
#[must_use]
pub fn id_of_store_bytes(bytes: &[u8]) -> Option<String> {
    session_format::read::ProjectFile::read(bytes)
        .ok()?
        .song_id()
        .map(str::to_owned)
}
