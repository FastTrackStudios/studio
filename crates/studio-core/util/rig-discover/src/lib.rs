//! Where Signal's rig engine is.
//!
//! The rig engine (`studio.rig`) binds loopback on a free port and says
//! where in `<runtime dir>/fts/rig-engines/<pid>.json`: its vox URL, its
//! pid, its host (the app that started it) and when it started. A rig view
//! takes the engine of its own app (itself when linked in, its parent as a
//! development copy), so two Studios on one machine never cross.
//! `STUDIO_RIG_URL` names an engine outright (another machine's).
//!
//! Two engines under one host (a second one started by hand, or a check
//! of the package) are told apart by when they started: the newest wins,
//! the same one for every view (studio#566).
//!
//! Both sides read this: the engine (an engine crate) writes it, the views
//! (connect crates) read it, so it lives in core.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The environment variable that names an engine outright.
pub const URL_ENV: &str = "STUDIO_RIG_URL";

/// An engine, as it says where it is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Found {
    /// Its vox endpoint (`ws://127.0.0.1:<port>/vox`).
    pub url: String,
    /// The engine's process.
    pub pid: u32,
    /// The app that started it.
    pub host: u32,
    /// When it started serving, milliseconds since the Unix epoch (0 in a
    /// file written before it was said).
    #[serde(default)]
    pub started: u64,
}

/// Where engines say where they are.
#[must_use]
pub fn dir() -> Option<PathBuf> {
    let base = dirs::runtime_dir().or_else(dirs::cache_dir)?;
    Some(base.join("fts").join("rig-engines"))
}

/// This process's parent (the app, for an extension's process).
#[cfg(unix)]
#[must_use]
pub fn parent() -> u32 {
    std::os::unix::process::parent_id()
}

/// This process's parent: unknown off Unix.
#[cfg(not(unix))]
#[must_use]
pub fn parent() -> u32 {
    0
}

/// Now, milliseconds since the Unix epoch.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// This process's engine file while it is kept: removed when dropped (the
/// engine stops serving).
#[derive(Debug)]
pub struct Published {
    path: PathBuf,
}

impl Published {
    /// The file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Published {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Says where this process's engine is (`url`), in [`dir`]. `None` when
/// the file could not be written.
#[must_use]
pub fn publish(url: &str) -> Option<Published> {
    publish_in(&dir()?, url)
}

/// [`publish`], into `dir`.
#[must_use]
pub fn publish_in(dir: &Path, url: &str) -> Option<Published> {
    std::fs::create_dir_all(dir).ok()?;
    let found = Found {
        url: url.to_owned(),
        pid: std::process::id(),
        host: parent(),
        started: now_ms(),
    };
    let path = dir.join(format!("{}.json", found.pid));
    std::fs::write(&path, serde_json::to_vec(&found).ok()?).ok()?;
    Some(Published { path })
}

/// Whether process `pid` is there.
#[cfg(unix)]
#[must_use]
pub fn alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 sends nothing; it only asks whether the process
    // exists.
    let gone = unsafe { libc::kill(pid, 0) } != 0
        && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
    !gone
}

/// Whether process `pid` is there: assumed so off Unix.
#[cfg(not(unix))]
#[must_use]
pub fn alive(_pid: u32) -> bool {
    true
}

/// Every engine that says it is up in [`dir`], a dead one's file removed.
#[must_use]
pub fn found() -> Vec<Found> {
    dir().map(|d| found_in(&d)).unwrap_or_default()
}

/// [`found`], in `dir`.
#[must_use]
pub fn found_in(dir: &Path) -> Vec<Found> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let found: Found = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
            if alive(found.pid) {
                Some(found)
            } else {
                let _ = std::fs::remove_file(path);
                None
            }
        })
        .collect()
}

/// The engine of the app whose processes `hosts` are, among `found`: the
/// newest when there are two (then the higher pid, so every view picks
/// the same one).
#[must_use]
pub fn of_hosts(found: &[Found], hosts: &[u32]) -> Option<Found> {
    found
        .iter()
        .filter(|f| hosts.contains(&f.host))
        .max_by_key(|f| (f.started, f.pid))
        .cloned()
}

/// The URL of this app's engine: `STUDIO_RIG_URL`, else the engine its
/// app started.
#[must_use]
pub fn url() -> Option<String> {
    if let Ok(url) = std::env::var(URL_ENV)
        && !url.trim().is_empty()
    {
        return Some(url.trim().to_owned());
    }
    of_hosts(&found(), &[std::process::id(), parent()]).map(|f| f.url)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{Found, found_in, of_hosts, publish_in};

    fn engine(pid: u32, host: u32, started: u64) -> Found {
        Found {
            url: format!("ws://127.0.0.1:{pid}/vox"),
            pid,
            host,
            started,
        }
    }

    #[test]
    fn a_view_takes_its_own_apps_engine() {
        let found = [engine(10, 1, 5), engine(20, 2, 5)];
        // Linked in: the view's process is the app.
        assert_eq!(of_hosts(&found, &[2, 900]).map(|f| f.pid), Some(20));
        // A development copy: its parent is the app.
        assert_eq!(of_hosts(&found, &[300, 1]).map(|f| f.pid), Some(10));
    }

    #[test]
    fn another_apps_engine_is_never_taken() {
        let found = [engine(10, 1, 5)];
        assert_eq!(of_hosts(&found, &[2, 3]), None);
    }

    #[test]
    fn of_two_engines_under_one_host_the_newest_wins_whatever_the_order() {
        let older = engine(30, 1, 100);
        let newer = engine(12, 1, 200);
        for found in [
            [older.clone(), newer.clone()],
            [newer.clone(), older.clone()],
        ] {
            assert_eq!(of_hosts(&found, &[1]).map(|f| f.pid), Some(12));
        }
        // Started in the same millisecond: the higher pid, every time.
        let twin = engine(40, 1, 200);
        assert_eq!(of_hosts(&[twin, newer], &[1]).map(|f| f.pid), Some(40));
    }

    #[test]
    fn a_file_without_its_start_still_reads() {
        let old: Found = serde_json::from_str(r#"{"url":"ws://x/vox","pid":7,"host":1}"#).unwrap();
        assert_eq!(old.started, 0);
    }

    #[test]
    fn a_published_engine_is_found_until_it_stops_serving() {
        let dir = tempfile::tempdir().unwrap();
        let published = publish_in(dir.path(), "ws://127.0.0.1:1/vox").unwrap();
        let found = found_in(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found.first().map(|f| f.pid), Some(std::process::id()));
        drop(published);
        assert!(found_in(dir.path()).is_empty());
    }
}
