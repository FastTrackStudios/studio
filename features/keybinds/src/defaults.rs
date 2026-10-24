//! The keymap studio ships, and the user's keymap file over it.
//!
//! Both are `keymap.styx` files written in the JSON subset of styx, so
//! they decode with [`KeymapConfig::from_json_str`]; architect-input reads
//! no files, so the reading happens here.

use std::path::{Path, PathBuf};

use architect_input::{ConfigError, KeymapConfig};

/// The shipped default keymap (`defaults/keymap.styx`).
pub const DEFAULT_KEYMAP: &str = include_str!("../defaults/keymap.styx");

/// Why a keymap file could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("failed to read keymap file {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error(transparent)]
    Config(#[from] ConfigError),
}

/// The shipped default keymap.
///
/// # Errors
///
/// [`LoadError::Config`] if the embedded file does not decode (a build
/// that ships a broken default; the tests below catch it).
pub fn load_default_config() -> Result<KeymapConfig, LoadError> {
    Ok(KeymapConfig::from_json_str(DEFAULT_KEYMAP)?)
}

/// Where the user's keymap lives on this OS, if there is a home to put
/// it in.
#[must_use]
pub fn default_user_config_path() -> Option<PathBuf> {
    user_config_path_in(&UserDirs {
        home: std::env::var_os("HOME").map(PathBuf::from),
        xdg_config_home: std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
    })
}

/// The user's keymap, from [`default_user_config_path`]. `Ok(None)` when
/// there is no such file.
///
/// # Errors
///
/// [`LoadError`] when the file exists but cannot be read or decoded.
pub fn load_user_config() -> Result<Option<KeymapConfig>, LoadError> {
    let Some(path) = default_user_config_path() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    load_keymap_file(&path).map(Some)
}

/// Read and decode one keymap file.
///
/// # Errors
///
/// [`LoadError::Io`] if it cannot be read, [`LoadError::Config`] if it
/// does not decode.
pub fn load_keymap_file(path: &Path) -> Result<KeymapConfig, LoadError> {
    let source = std::fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.to_owned(),
        source,
    })?;
    Ok(KeymapConfig::from_json_str(&source)?)
}

/// The environment [`user_config_path_in`] reads.
struct UserDirs {
    home: Option<PathBuf>,
    xdg_config_home: Option<PathBuf>,
}

#[cfg(target_os = "macos")]
fn user_config_path_in(dirs: &UserDirs) -> Option<PathBuf> {
    let base = dirs.home.as_ref()?;
    Some(base.join("Library/Application Support/FastTrackStudio/keymap.styx"))
}

#[cfg(target_os = "linux")]
fn user_config_path_in(dirs: &UserDirs) -> Option<PathBuf> {
    let config = match (&dirs.xdg_config_home, &dirs.home) {
        (Some(xdg), _) => xdg.clone(),
        (None, Some(home)) => home.join(".config"),
        (None, None) => return None,
    };
    Some(config.join("fasttrackstudio/keymap.styx"))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn user_config_path_in(_dirs: &UserDirs) -> Option<PathBuf> {
    None
}
