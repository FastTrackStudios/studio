//! Where the rig engine is.
//!
//! `studio.rig` binds loopback on a free port and says where in
//! `<runtime dir>/fts/rig-engines/<pid>.json`: its vox URL, its pid and its
//! host (the app that started it). A view takes the engine of its own app
//! (itself when linked in, its parent as a development copy), so two
//! Studios on one machine never cross. `STUDIO_RIG_URL` names an engine
//! outright (another machine's, `studio-desktop --engine`'s).

use std::path::PathBuf;

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
}

/// Where engines say where they are.
#[must_use]
pub fn dir() -> Option<PathBuf> {
    let base = dirs::runtime_dir().or_else(dirs::cache_dir)?;
    Some(base.join("fts").join("rig-engines"))
}

/// This process's parent (the app, for an extension's process).
#[must_use]
pub fn parent() -> u32 {
    std::os::unix::process::parent_id()
}

/// Says where this process's engine is. The file, or `None` when it
/// could not be written.
#[must_use]
pub fn publish(url: &str) -> Option<PathBuf> {
    let dir = dir()?;
    std::fs::create_dir_all(&dir).ok()?;
    let found = Found {
        url: url.to_owned(),
        pid: std::process::id(),
        host: parent(),
    };
    let path = dir.join(format!("{}.json", found.pid));
    std::fs::write(&path, serde_json::to_vec(&found).ok()?).ok()?;
    Some(path)
}

/// Whether process `pid` is there.
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

/// Every engine that says it is up, a dead one's file removed.
#[must_use]
pub fn found() -> Vec<Found> {
    let Some(dir) = dir() else {
        return Vec::new();
    };
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

/// The engine of the app whose processes `hosts` are, among `found`.
#[must_use]
pub fn of_hosts(found: &[Found], hosts: &[u32]) -> Option<Found> {
    found.iter().find(|f| hosts.contains(&f.host)).cloned()
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
mod tests {
    use super::{Found, of_hosts};

    fn engine(pid: u32, host: u32) -> Found {
        Found {
            url: format!("ws://127.0.0.1:{pid}/vox"),
            pid,
            host,
        }
    }

    #[test]
    fn a_view_takes_its_own_apps_engine() {
        let found = [engine(10, 1), engine(20, 2)];
        // Linked in: the view's process is the app.
        assert_eq!(of_hosts(&found, &[2, 900]).map(|f| f.pid), Some(20));
        // A development copy: its parent is the app.
        assert_eq!(of_hosts(&found, &[300, 1]).map(|f| f.pid), Some(10));
    }

    #[test]
    fn another_apps_engine_is_never_taken() {
        let found = [engine(10, 1)];
        assert_eq!(of_hosts(&found, &[2, 3]), None);
    }
}
