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
use std::time::Instant;

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
pub fn home() -> Option<PathBuf> {
    static HOME: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    HOME.get_or_init(find_home).clone()
}

fn find_home() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("STUDIO_HOME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(d));
    }
    let config = config_dir()?.join("fasttrackstudio");
    if let Some(h) = std::fs::read_to_string(config.join("studio.toml"))
        .ok()
        .and_then(|t| home_setting(&t))
    {
        return Some(expand_home(&h));
    }
    let mount = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Drive").join("Studio"));
    if let Some(m) = mount.filter(|m| m.is_dir()) {
        return Some(m);
    }
    Some(config.join("Studio"))
}

/// `home = "…"` in the settings file (a line of TOML; nothing else is read).
fn home_setting(toml: &str) -> Option<String> {
    toml.lines()
        .find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == "home")
                .then(|| v.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
        })
        .filter(|v| !v.is_empty())
}

fn expand_home(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(h)) => PathBuf::from(h).join(rest),
        _ => PathBuf::from(p),
    }
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

pub fn config_dir() -> Option<PathBuf> {
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
        assert!(d.ends_with("fasttrackstudio/studio-app"), "{}", d.display());
    }

    #[test]
    fn the_home_setting() {
        assert_eq!(
            super::home_setting("# Studio\nhome = \"/home/me/Drive/Studio\"\n").as_deref(),
            Some("/home/me/Drive/Studio")
        );
        assert_eq!(super::home_setting("theme = 'dark'\n"), None);
        assert_eq!(super::home_setting("home = ''"), None);
    }
}
