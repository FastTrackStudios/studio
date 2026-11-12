//! Studio's island as the node ships it (#445), checked against its lock.
//!
//! `just island`'s folder (`apps/architect/island`, built by Architect's
//! `build-web`) is read from disk and checked before the node serves any
//! of it.
//!
//! The folder's `architect.lock.json` names every file a page loads from
//! it with its `sha256-…` (base64) digest, written at build time: the
//! component's files and its stylesheet ([`STYLESHEET`]; `just island`
//! writes the sheet into the folder and runs `build-web` again, which
//! locks every file it finds there). The island is refused whole, and the
//! node then has no island (a share link's now-playing view keeps
//! Architect's own bar and controls), when:
//! - the folder, the lock or a file the lock names is not a regular file
//!   (or, for the folder, directory) of its own: a symlink, a directory, a
//!   device or a pipe is refused, never followed;
//! - the lock is unreadable, names nothing, does not name the stylesheet,
//!   or names anything but a plain file name directly in the folder;
//! - a file it names is missing, or its bytes are not its digest.
//!
//! Only the files the lock names and the lock itself are served; anything
//! else in the folder (`BUILT_FROM`, build-web's marker) is not.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::Path;

use architect_sites::SiteIsland;
use base64::Engine as _;
use sha2::{Digest as _, Sha256};

/// The island's extension id (`apps/architect/island/extension.toml`).
pub const ID: &str = "studio.transport";

/// Its lock, as `build-web` writes it.
pub const LOCK: &str = "architect.lock.json";

/// Its stylesheet, as `just island` writes it into the folder before the
/// lock is written: the lock names it.
pub const STYLESHEET: &str = "island.css";

/// What `just island` built it from (`git describe --always --dirty`),
/// which the release checks against its own (scripts/studio-architect.sh).
/// Not served.
pub const BUILT_FROM: &str = "BUILT_FROM";

/// The largest file the island may hold (its core module is under 1 MB).
const MAX_FILE: u64 = 16 * 1024 * 1024;

/// Why a folder is not served as the island.
#[derive(Debug, thiserror::Error)]
pub enum IslandError {
    /// No folder, or no lock in it: nothing shipped (not a refusal).
    #[error("no {LOCK} in {0}")]
    Absent(String),
    #[error("{LOCK} is unreadable: {0}")]
    Lock(String),
    #[error("{LOCK} names a path that is not a plain file name in the folder: {0}")]
    Path(String),
    #[error("{0} is missing")]
    Missing(String),
    #[error("{0} is not a regular file of its own (a link, a folder or a special file)")]
    NotAFile(String),
    #[error("{0} is not what {LOCK} says")]
    Altered(String),
    #[error("{LOCK} does not name the stylesheet {STYLESHEET}")]
    Unlocked,
}

#[derive(serde::Deserialize)]
struct Lock {
    files: BTreeMap<String, String>,
}

/// `sha256-<base64>` of `bytes`, the lock's form.
fn digest(bytes: &[u8]) -> String {
    format!(
        "sha256-{}",
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(bytes))
    )
}

/// A name the lock may give: one plain file name, directly in the folder
/// (build-web's output is flat), never `.`/`..`, a separator or a NUL.
fn plain(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
}

/// The bytes of `name` in `dir`, if it is a regular file of its own: not
/// followed through a symlink, not a folder or a special file. What is read
/// is the file that was checked (the open file's own metadata is compared
/// with the name's), so swapping the name between the two does not serve
/// another file.
fn regular(dir: &Path, name: &str) -> Result<Vec<u8>, IslandError> {
    let path = dir.join(name);
    let at = std::fs::symlink_metadata(&path).map_err(|_| IslandError::Missing(name.to_owned()))?;
    if !at.file_type().is_file() || at.len() > MAX_FILE {
        return Err(IslandError::NotAFile(name.to_owned()));
    }
    let mut file = std::fs::File::open(&path).map_err(|_| IslandError::Missing(name.to_owned()))?;
    let opened = file
        .metadata()
        .map_err(|_| IslandError::Missing(name.to_owned()))?;
    if !opened.file_type().is_file() || !same_file(&at, &opened) {
        return Err(IslandError::NotAFile(name.to_owned()));
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_FILE)
        .read_to_end(&mut bytes)
        .map_err(|_| IslandError::Missing(name.to_owned()))?;
    Ok(bytes)
}

#[cfg(unix)]
fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(not(unix))]
fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    a.len() == b.len()
}

/// The island in `dir`, every file its lock names checked against it.
///
/// # Errors
///
/// [`IslandError`]: no folder or no lock ([`IslandError::Absent`]), or a
/// folder that is not exactly what its lock says.
pub fn at(dir: &Path) -> Result<SiteIsland, IslandError> {
    let folder = dir.display().to_string();
    match std::fs::symlink_metadata(dir) {
        Err(_) => return Err(IslandError::Absent(folder)),
        Ok(m) if !m.file_type().is_dir() => return Err(IslandError::NotAFile(folder)),
        Ok(_) => {}
    }
    let lock_bytes = match regular(dir, LOCK) {
        Err(IslandError::Missing(_)) => return Err(IslandError::Absent(folder)),
        other => other?,
    };
    let lock: Lock =
        serde_json::from_slice(&lock_bytes).map_err(|e| IslandError::Lock(e.to_string()))?;
    if lock.files.is_empty() {
        return Err(IslandError::Lock("it names no files".to_owned()));
    }
    if !lock.files.contains_key(STYLESHEET) {
        return Err(IslandError::Unlocked);
    }
    // Every name first: nothing is read for a lock that names a path.
    if let Some(name) = lock.files.keys().find(|n| !plain(n) || *n == LOCK) {
        return Err(IslandError::Path(name.clone()));
    }
    let mut files: Vec<(String, Cow<'static, [u8]>)> =
        Vec::with_capacity(lock.files.len().saturating_add(1));
    for (name, want) in &lock.files {
        let bytes = regular(dir, name)?;
        if digest(&bytes) != *want {
            return Err(IslandError::Altered(name.clone()));
        }
        files.push((name.clone(), Cow::Owned(bytes)));
    }
    files.push((LOCK.to_owned(), Cow::Owned(lock_bytes)));
    // Its web bundle (the `web-*` files `just island` writes) is found by
    // its files; its `lyrics` view is the web bundle's only (the share
    // page's Lyrics tab, Session's lyrics panel as its Confidence Monitor).
    Ok(SiteIsland::new(ID, files)
        .progress("progress")
        .transport("transport")
        .lyrics("lyrics")
        .stylesheet(STYLESHEET))
}
