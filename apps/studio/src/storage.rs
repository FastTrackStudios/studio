//! Where Studio keeps what it keeps, natively: one folder per user,
//! `<config dir>/fasttrackstudio/studio` (`$XDG_CONFIG_HOME` or
//! `~/.config` on Linux, `~/Library/Application Support` on macOS,
//! `%APPDATA%` on Windows). Studio works without Drive: its sets, songs and
//! the org it used last are copies in this folder, read back on the next
//! start and sent to a node when one answers. With Drive installed, the
//! folder can be one of Drive's locations, synced like any other.
//!
//! Every path Studio writes goes through here, so a store of plain files
//! (a set a file, a song a folder) can take the documents' place behind
//! this module without the pages knowing.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;

static START: OnceLock<Instant> = OnceLock::new();

/// Studio's folder: `STUDIO_DATA_DIR` when set (a second instance, a
/// test), else the platform's config folder's `fasttrackstudio/studio`.
pub fn dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("STUDIO_DATA_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(d));
    }
    config_dir().map(|c| c.join("fasttrackstudio").join("studio"))
}

fn config_dir() -> Option<PathBuf> {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(target_os = "windows") {
        env("APPDATA")
    } else if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
        env("HOME").map(|h| h.join("Library").join("Application Support"))
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))
    }
}

/// Called first in `main`: the clock for the start-up span, and Drive's
/// client keeps its copies (documents, who this is, the last org) in
/// Studio's folder, under `drive/`.
pub fn init() {
    START.get_or_init(Instant::now);
    #[cfg(all(feature = "drive", not(target_arch = "wasm32")))]
    if let Some(d) = dir() {
        drive_client::set_data_dir(d.join("drive"));
    }
}

/// Milliseconds since `init`.
pub fn since_start_ms() -> u64 {
    START.get().map_or(0, |s| {
        u64::try_from(s.elapsed().as_millis()).unwrap_or(u64::MAX)
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn studio_has_one_folder_under_the_config_dir() {
        if std::env::var_os("STUDIO_DATA_DIR").is_some() {
            return;
        }
        let d = super::dir().expect("a home");
        assert!(d.ends_with("fasttrackstudio/studio"), "{}", d.display());
    }
}
