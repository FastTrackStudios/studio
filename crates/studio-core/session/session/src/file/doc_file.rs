//! A session's own document file on disk: the header, then records
//! ([`super::format`]), appended one step at a time and rewritten whole
//! when it is folded.
//!
//! The layout is the one Architect's `crdt::file::DocFile` writes (ported
//! from it, MIT OR Apache-2.0, `FastTrackStudios`), so every session and
//! chart file made before opens as it was. Studio edits `main` only;
//! other branches an old file holds are read ([`read_branches`]) and kept
//! through a fold.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use loro::LoroDoc;
use uuid::Uuid;

use super::FileError;
use super::format::{self, Record};
use super::index::Index;

/// The branch every document starts with, and the one Studio edits.
pub const MAIN: &str = "main";

/// What a step written by the session's own writer is named: its commits
/// are not named one by one, and are not versions to open.
const STEP_LABEL: &str = "Sync";

/// Milliseconds since the epoch: when a step or snapshot was written.
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

// By value: an error-mapping adapter for `map_err`.
#[allow(clippy::needless_pass_by_value)]
fn io(e: std::io::Error) -> FileError {
    FileError::Storage(format!("fs: {e}"))
}

fn meta(key: &str, value: &str) -> Record {
    Record::Meta {
        key: key.to_owned(),
        value: value.to_owned(),
    }
}

/// A whole file of `records`.
fn image(records: &[Record]) -> Result<Vec<u8>, FileError> {
    let mut bytes = format::header();
    for r in records {
        bytes.extend(format::frame(r)?);
    }
    Ok(bytes)
}

/// One session's document file, open for writing. One writer at a time:
/// two processes appending to one file would interleave their records.
#[derive(Debug)]
pub struct DocFile {
    path: PathBuf,
    file: File,
    index: Index,
}

impl DocFile {
    /// Makes a document of kind `kind` for `id` at `path` (whatever was
    /// there is replaced), its `main` empty. `schema` is the stamp it is
    /// made at; `None` for a document adopted from another device, whose
    /// stamp arrives with its history.
    pub fn create(
        path: &Path,
        kind: &str,
        id: Uuid,
        schema: Option<u32>,
    ) -> Result<Self, FileError> {
        let mut records = vec![meta("kind", kind)];
        if let Some(schema) = schema {
            records.push(meta("schema", &schema.to_string()));
        }
        records.extend([
            meta("id", &id.to_string()),
            Record::Branch {
                name: MAIN.to_owned(),
                parent: None,
                base: None,
                at: now(),
            },
        ]);
        replace(path, &image(&records)?)?;
        Ok(Self {
            path: path.to_path_buf(),
            file: open(path)?,
            index: Index::from_records(records),
        })
    }

    /// Opens the document at `path`, dropping a torn tail a crash left.
    ///
    /// # Errors
    ///
    /// Not a document file, or not of `kind`, or from a newer build than
    /// `schema`.
    pub fn open(path: &Path, kind: &str, schema: u32) -> Result<Self, FileError> {
        let mut file = open(path)?;
        let mut bytes = Vec::new();
        file.seek(SeekFrom::Start(0)).map_err(io)?;
        file.read_to_end(&mut bytes).map_err(io)?;
        let parsed = format::parse(&bytes)?;
        if parsed.torn(bytes.len()) > 0 {
            let good = u64::try_from(parsed.good_len).unwrap_or(u64::MAX);
            file.set_len(good).map_err(io)?;
            file.sync_all().map_err(io)?;
        }
        let index = Index::from_records(parsed.records);
        check(&index, kind, schema)?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
            index,
        })
    }

    /// The document's id (made with it, kept in `meta`).
    pub fn id(&self) -> Option<Uuid> {
        self.index.meta("id").and_then(|s| s.parse().ok())
    }

    /// `main` as the file holds it.
    pub fn main(&self) -> Result<LoroDoc, FileError> {
        self.index.materialize(MAIN)
    }

    /// Appends `update` to `main` as one step, durable when it returns.
    pub fn append(&mut self, update: &[u8]) -> Result<(), FileError> {
        let record = Record::Step {
            seq: self.index.next_seq(),
            branch: MAIN.to_owned(),
            label: STEP_LABEL.to_owned(),
            at: now(),
            // None written: not a version to open (a raw update).
            frontiers: Vec::new(),
            data: update.to_vec(),
        };
        let bytes = format::frame(&record)?;
        self.file.seek(SeekFrom::End(0)).map_err(io)?;
        self.file.write_all(&bytes).map_err(io)?;
        self.file.sync_data().map_err(io)?;
        self.index.apply(record);
        Ok(())
    }

    /// Folds `main` into `snapshot` (the whole document, which holds every
    /// step the file does) and rewrites the file to what it holds.
    pub fn fold(&mut self, snapshot: Vec<u8>) -> Result<(), FileError> {
        let mut folded = self.index.clone();
        folded.apply(Record::Snapshot {
            branch: MAIN.to_owned(),
            upto: self.index.last_step(MAIN),
            at: now(),
            data: snapshot,
        });
        replace(&self.path, &image(&folded.records())?)?;
        self.file = open(&self.path)?;
        self.index = folded;
        Ok(())
    }
}

/// Refuses a file of another kind, or one a newer build wrote.
fn check(index: &Index, kind: &str, schema: u32) -> Result<(), FileError> {
    let found = index.meta("kind").unwrap_or_default();
    if found != kind {
        return Err(FileError::WrongKind {
            found: found.to_owned(),
            expected: kind.to_owned(),
        });
    }
    let stamp: u32 = index
        .meta("schema")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    if stamp > schema {
        return Err(FileError::Schema(format!(
            "this session was written by a newer version (schema {stamp}, this build reads {schema})"
        )));
    }
    Ok(())
}

/// A document file's id and every branch's document, read from its bytes
/// (the file itself is not opened): what a converter reads an old file by.
///
/// # Errors
///
/// Not a document file of `kind`, or a branch that would not load.
pub fn read_branches(
    bytes: &[u8],
    kind: &str,
) -> Result<(Uuid, Vec<(String, LoroDoc)>), FileError> {
    let index = Index::from_records(format::parse(bytes)?.records);
    let found = index.meta("kind").unwrap_or_default();
    if found != kind {
        return Err(FileError::WrongKind {
            found: found.to_owned(),
            expected: kind.to_owned(),
        });
    }
    let id = index
        .meta("id")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| FileError::NotADocument("no id".into()))?;
    let branches = index
        .branches()
        .into_iter()
        .map(|name| index.materialize(&name).map(|doc| (name, doc)))
        .collect::<Result<_, _>>()?;
    Ok((id, branches))
}

fn open(path: &Path) -> Result<File, FileError> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(io)
}

/// Replaces the file at `path` with `bytes`, atomically: written beside
/// it and renamed over, so a crash leaves the old file or the new one.
fn replace(path: &Path, bytes: &[u8]) -> Result<(), FileError> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    {
        let mut out = File::create(&tmp).map_err(io)?;
        out.write_all(bytes).map_err(io)?;
        out.sync_all().map_err(io)?;
    }
    std::fs::rename(&tmp, path).map_err(io)?;
    // The rename itself is durable once the directory is synced.
    #[cfg(unix)]
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        File::open(dir).and_then(|d| d.sync_all()).map_err(io)?;
    }
    Ok(())
}

/// The `main` the file `bytes` hold, read from the bytes alone.
#[cfg(test)]
pub fn main_of(bytes: &[u8]) -> Result<LoroDoc, FileError> {
    Index::from_records(format::parse(bytes)?.records).materialize(MAIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_survives_a_torn_write_after_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.session");
        let id = Uuid::from_u128(7);
        let mut file = DocFile::create(&path, "fts.session", id, Some(1)).unwrap();
        let doc = LoroDoc::new();
        let _ = doc.get_text("t").insert(0, "kept");
        doc.commit();
        let update = doc.export(loro::ExportMode::all_updates()).unwrap();
        file.append(&update).unwrap();
        drop(file);
        // Half a record more: a crash in the middle of the next write.
        let mut bytes = std::fs::read(&path).unwrap();
        let whole = bytes.len();
        bytes.extend_from_slice(&[9, 0, 0, 0, 5, 1]);
        std::fs::write(&path, &bytes).unwrap();

        let file = DocFile::open(&path, "fts.session", 1).unwrap();
        assert_eq!(file.id(), Some(id));
        let main = file.main().unwrap();
        assert_eq!(main.get_text("t").to_string(), "kept");
        assert_eq!(
            std::fs::metadata(&path).map(|m| m.len()).ok(),
            u64::try_from(whole).ok(),
            "the torn tail is cut off"
        );
    }

    #[test]
    fn a_fold_keeps_the_document_and_drops_its_steps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.session");
        let mut file = DocFile::create(&path, "fts.session", Uuid::from_u128(1), Some(1)).unwrap();
        let doc = LoroDoc::new();
        for word in ["one ", "two ", "three"] {
            let text = doc.get_text("t");
            let _ = text.insert(text.len_unicode(), word);
            let before = doc.oplog_vv();
            doc.commit();
            let update = doc.export(loro::ExportMode::updates(&before)).unwrap();
            file.append(&update).unwrap();
        }
        file.fold(doc.export(loro::ExportMode::Snapshot).unwrap())
            .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let main = main_of(&bytes).unwrap();
        assert_eq!(main.get_text("t").to_string(), "one two three");
        assert_eq!(
            file.main().unwrap().get_text("t").to_string(),
            "one two three"
        );
    }

    #[test]
    fn another_kind_or_a_newer_build_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.note");
        drop(DocFile::create(
            &path,
            "task.note",
            Uuid::from_u128(1),
            Some(1),
        ));
        assert!(matches!(
            DocFile::open(&path, "fts.session", 1),
            Err(FileError::WrongKind { .. })
        ));
        let path = dir.path().join("b.session");
        drop(DocFile::create(
            &path,
            "fts.session",
            Uuid::from_u128(1),
            Some(3),
        ));
        assert!(matches!(
            DocFile::open(&path, "fts.session", 1),
            Err(FileError::Schema(_))
        ));
    }
}
