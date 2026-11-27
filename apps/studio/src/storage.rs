//! Where Studio keeps things, natively: one plain folder, its home
//! (`docs/storage.md`). Its sets are `.setlist` files in
//! `<home>/Setlists`; its songs folders in `<home>/Songs` (or wherever a
//! set's lines point); edit them by hand and Studio follows. The home is,
//! in order: `STUDIO_HOME`; `home = "…"` in
//! `<config dir>/fasttrackstudio/studio.toml`; `~/Drive/Studio` when the
//! Drive mount is there; `<config dir>/fasttrackstudio/Studio`
//! (`$XDG_CONFIG_HOME` or `~/.config` on Linux, `~/Library/Application
//! Support` on macOS, `%APPDATA%` on Windows). Without Drive it is an
//! ordinary folder, used the same way.
//!
//! Drive's client keeps its copies of documents (the app's own state, not
//! for editing) apart from the home, in
//! `<config dir>/fasttrackstudio/studio-app/drive`.
//!
//! Every path Studio reads or writes is found through here.

// A browser has no disk: what reads and writes it is the native build's.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use std::path::PathBuf;
use std::sync::OnceLock;
use web_time::Instant;

static START: OnceLock<Instant> = OnceLock::new();

/// The app's own state (Drive's copies), not for editing:
/// `STUDIO_DATA_DIR` when set (a second instance, a test).
pub fn dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("STUDIO_DATA_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(d));
    }
    config_dir().map(|c| c.join("fasttrackstudio").join("studio-app"))
}

/// The home: where the sets and songs are (see the module's doc). Chosen
/// once a run: a Drive mount that drops out while Studio runs leaves an
/// empty mount point, and the home must not move to (or be made in) it.
/// The engine finds the same home (`utils::home`), and Signal's in it.
pub fn home() -> Option<PathBuf> {
    utils::home::studio_home()
}

/// `<home>/Setlists`: one `.setlist` file a set.
pub fn setlists() -> Option<PathBuf> {
    home().map(|r| r.join("Setlists"))
}

/// A path in the home as a path on disk: an absolute one as it is.
pub fn resolve(path: &str) -> Option<PathBuf> {
    if path.starts_with('/') {
        return Some(PathBuf::from(path));
    }
    home().map(|r| r.join(path))
}

pub use utils::home::config_dir;

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
        assert!(d.ends_with("fasttrackstudio/studio-app"), "{}", d.display());
    }

    #[test]
    fn the_home_setting() {
        assert_eq!(
            utils::home::home_setting("# Studio\nhome = \"/home/me/Drive/Studio\"\n").as_deref(),
            Some("/home/me/Drive/Studio")
        );
        assert_eq!(utils::home::home_setting("theme = 'dark'\n"), None);
        assert_eq!(utils::home::home_setting("home = ''"), None);
    }
}
