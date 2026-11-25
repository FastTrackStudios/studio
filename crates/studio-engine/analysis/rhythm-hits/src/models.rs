//! License-aware model pack manifest (`models.json`) and local cache resolution.
//!
//! Keeps the open-source repository free of large binary weights and cleanly
//! separates built-in / permissive (MIT, Apache-2.0) models from optional
//! user-downloaded non-commercial (`CC BY-NC` / academic) model packs.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LicenseTier {
    /// Zero-weight pure DSP built into the binary.
    BuiltIn,
    /// OSI-permissive weights (MIT / Apache-2.0) safe for redistribution.
    Permissive,
    /// Non-commercial or academic weights (e.g., CC BY-NC 4.0); user-fetched only.
    NonCommercial,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelSpec {
    pub id: String,
    pub name: String,
    pub task: String,
    pub license: String,
    pub tier: LicenseTier,
    pub filename: String,
    pub url: String,
    #[serde(default)]
    pub sha256: Option<String>,
    pub sample_rate_hz: u32,
    #[serde(default)]
    pub notes: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelManifest {
    pub version: u32,
    pub models: Vec<ModelSpec>,
}

impl ModelManifest {
    pub fn from_json_str(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| format!("invalid models.json: {e}"))
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let p = path.as_ref();
        let text = std::fs::read_to_string(p)
            .map_err(|e| format!("read {}: {e}", p.display()))?;
        Self::from_json_str(&text)
    }

    pub fn find(&self, id: &str) -> Option<&ModelSpec> {
        self.models.iter().find(|m| m.id == id)
    }
}

/// Default local model cache directory.
///
/// Precedence:
/// 1. `FTS_RHYTHM_MODEL_DIR`
/// 2. `$XDG_CACHE_HOME/fts-rhythm/models`
/// 3. `$HOME/.cache/fts-rhythm/models`
#[must_use]
pub fn default_cache_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("FTS_RHYTHM_MODEL_DIR")
        && !dir.trim().is_empty()
    {
        return PathBuf::from(dir);
    }
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME")
        && !xdg.trim().is_empty()
    {
        return PathBuf::from(xdg).join("fts-rhythm").join("models");
    }
    if let Ok(home) = std::env::var("HOME")
        && !home.trim().is_empty()
    {
        return PathBuf::from(home)
            .join(".cache")
            .join("fts-rhythm")
            .join("models");
    }
    PathBuf::from(".cache/fts-rhythm/models")
}

/// Resolve a model file inside `cache_dir` (or [`default_cache_dir`]),
/// returning a clear download hint if the weight file is not cached yet.
pub fn resolve_cached_model(
    spec: &ModelSpec,
    cache_dir: Option<&Path>,
) -> Result<PathBuf, String> {
    let dir = cache_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(default_cache_dir);
    let path = dir.join(&spec.filename);
    if path.is_file() {
        return Ok(path);
    }
    Err(format!(
        "model '{}' ({}, license: {}) not found at {}. Download from {} into {}",
        spec.id,
        spec.name,
        spec.license,
        path.display(),
        spec.url,
        dir.display(),
    ))
}
