//! Studio's island as the node ships it (#445), checked against its lock.
//!
//! `just island`'s folder (`apps/architect/island`, built by Architect's
//! `build-web`) is read from disk and checked before the node serves any
//! of it.
//!
//! The folder's `architect.lock.json` names every file the component is
//! made of with its `sha256-…` (base64) digest. A folder whose lock is
//! missing or unreadable, names a path outside the folder, or names a file
//! that is missing or whose bytes differ, is refused whole: the node then
//! has no island, and a share link's now-playing view keeps Architect's
//! own bar and controls. Only the files the lock names, the lock itself
//! and the stylesheet ([`STYLESHEET`], pinned by the page on its own
//! digest) are served; anything else in the folder is not.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;

use architect_sites::SiteIsland;
use base64::Engine as _;
use sha2::{Digest as _, Sha256};

/// The island's extension id (`apps/architect/island/extension.toml`).
pub const ID: &str = "studio.transport";

/// Its lock, as `build-web` writes it.
pub const LOCK: &str = "architect.lock.json";

/// Its stylesheet, as `just island` writes it beside the folder's files.
pub const STYLESHEET: &str = "island.css";

/// The commit `just island` built it from (`git rev-parse HEAD`), which
/// the release checks against its own (scripts/studio-architect.sh).
/// Not served.
pub const BUILT_FROM: &str = "BUILT_FROM";

/// Why a folder is not served as the island.
#[derive(Debug, thiserror::Error)]
pub enum IslandError {
    /// No folder, or no lock in it: nothing shipped (not a refusal).
    #[error("no {LOCK} in {0}")]
    Absent(String),
    #[error("{LOCK} is unreadable: {0}")]
    Lock(String),
    #[error("{LOCK} names a path outside the folder: {0}")]
    Path(String),
    #[error("{0} is missing")]
    Missing(String),
    #[error("{0} is not what {LOCK} says")]
    Altered(String),
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

/// A name the lock may give: a plain relative path, inside the folder.
fn inside(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && name
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// The island in `dir`, every file its lock names checked against it.
///
/// # Errors
///
/// [`IslandError`]: no lock ([`IslandError::Absent`]), or a lock the
/// folder does not match.
pub fn at(dir: &Path) -> Result<SiteIsland, IslandError> {
    let lock_bytes = std::fs::read(dir.join(LOCK))
        .map_err(|_| IslandError::Absent(dir.display().to_string()))?;
    let lock: Lock =
        serde_json::from_slice(&lock_bytes).map_err(|e| IslandError::Lock(e.to_string()))?;
    if lock.files.is_empty() {
        return Err(IslandError::Lock("it names no files".to_owned()));
    }
    let mut files: Vec<(String, Cow<'static, [u8]>)> =
        Vec::with_capacity(lock.files.len().saturating_add(2));
    for (name, want) in &lock.files {
        if !inside(name) {
            return Err(IslandError::Path(name.clone()));
        }
        let bytes =
            std::fs::read(dir.join(name)).map_err(|_| IslandError::Missing(name.clone()))?;
        if digest(&bytes) != *want {
            return Err(IslandError::Altered(name.clone()));
        }
        files.push((name.clone(), Cow::Owned(bytes)));
    }
    files.push((LOCK.to_owned(), Cow::Owned(lock_bytes)));
    let sheet = std::fs::read(dir.join(STYLESHEET)).ok();
    let has_sheet = sheet.is_some();
    if let Some(sheet) = sheet {
        files.push((STYLESHEET.to_owned(), Cow::Owned(sheet)));
    }
    let island = SiteIsland::new(ID, files)
        .progress("progress")
        .transport("transport");
    Ok(if has_sheet {
        island.stylesheet(STYLESHEET)
    } else {
        island
    })
}
