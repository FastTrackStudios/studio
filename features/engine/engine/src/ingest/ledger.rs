//! What ingest sent from this machine, per root: each file's size, time
//! and hash when it went, and the address the server gave it — so a run
//! over an unchanged folder reads no file and sends nothing, and an
//! upload cut short resumes where it stopped.
//!
//! Kept in `$XDG_DATA_HOME/fts/ingest/<server>/<org>/<root>.json`. Losing
//! it costs a re-hash of each file, never a re-send of one the server
//! has (see `super::plan`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::plan::Kept;

/// One root's record.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Ledger {
    pub files: BTreeMap<String, Kept>,
}

/// Where the ledger of `root` on `server`'s `org` lives under `data`.
#[must_use]
pub fn path(data: &Path, server: &str, org: &str, root: &str) -> PathBuf {
    let host: String = server
        .split("://")
        .last()
        .unwrap_or(server)
        .trim_end_matches('/')
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    data.join("fts")
        .join("ingest")
        .join(host)
        .join(org)
        .join(format!("{root}.json"))
}

impl Ledger {
    /// The ledger at `path`; empty when there is none or it does not read
    /// (a ledger is a cache: the worst a lost one costs is hashing).
    #[must_use]
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Write it to `path`, whole, through a file beside it.
    ///
    /// # Errors
    ///
    /// The folder could not be made or the file written.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let partial = path.with_extension("json.partial");
        std::fs::write(&partial, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(partial, path)
    }
}
