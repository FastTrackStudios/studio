//! Where a song's session is on this machine, and the rigs on it.
//!
//! The one part of the launcher that reads the disk. A vault holds notes
//! (the light layer, `<org>/Vault`); the sessions are in the org's project
//! root beside it (the heavy layer, `<org>/Projects/<name>`), which no
//! vault action lists. Until a song's note names its session (`session`)
//! everywhere, the session is found by the song's name. A browser has no
//! such disk: there every function here finds nothing.

use std::path::{Path, PathBuf};

use crate::model::slug;

/// The org's project root for `project`: `Projects/<project>` beside the
/// vault's folder (`<org>/Vault` → `<org>/Projects/<project>`).
#[must_use]
pub fn project_root(vault_root: &Path, project: &str) -> PathBuf {
    vault_root
        .parent()
        .unwrap_or(vault_root)
        .join("Projects")
        .join(project)
}

/// The file a note's `session` names, when it is there: an absolute path,
/// else one in the project root, else one in the vault.
#[must_use]
pub fn resolve(written: &str, vault_root: &Path, project_root: &Path) -> Option<PathBuf> {
    let path = Path::new(written);
    if path.is_absolute() {
        return path.exists().then(|| path.to_owned());
    }
    if written.split('/').any(|part| part == "..") {
        return None;
    }
    [project_root.join(path), vault_root.join(path)]
        .into_iter()
        .find(|p| p.exists())
}

/// The folders a project root keeps songs in: its own and `songs/`'s.
fn song_folders(project_root: &Path) -> Vec<PathBuf> {
    let dirs = |at: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(at)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect()
    };
    let mut all = dirs(&project_root.join("songs"));
    all.extend(dirs(project_root));
    all
}

/// Whether folder slug `folder` is song slug `song`'s: the same, or the
/// song's whole-word tail (`cinematic-intro` for `dtla-cinematic-intro`).
fn names(folder: &str, song: &str) -> bool {
    !folder.is_empty() && (folder == song || song.ends_with(&format!("-{folder}")))
}

/// The session in a song folder: its `.session` (the shortest name), else
/// its `.RPP`.
fn session_in(dir: &Path) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    files.sort_by_key(|p| p.as_os_str().len());
    let with = |ext: &str| {
        files
            .iter()
            .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)))
            .cloned()
    };
    with("session").or_else(|| with("rpp"))
}

/// The session of song `title` in `project_root`, found by name: the
/// folder named for the song (`songs/drive-to-la`, `unbreakable`), then
/// its `.session` or `.RPP`.
#[must_use]
pub fn song_session(project_root: &Path, title: &str) -> Option<PathBuf> {
    let want = slug(title);
    song_folders(project_root)
        .into_iter()
        .filter(|dir| {
            dir.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| names(&slug(n), &want))
        })
        .find_map(|dir| session_in(&dir))
}

/// The setlist's own file in `project_root` (`<name>.setlist`, at its top
/// or in `songs/`), when it has one.
#[must_use]
pub fn setlist_file(project_root: &Path) -> Option<PathBuf> {
    [project_root.to_owned(), project_root.join("songs")]
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "setlist"))
}

/// The lighting cues of the song a setlist calls `label`: the vault's
/// `Projects/<project>/lighting/cues/<slug>.json` (Ignition's cue file).
#[must_use]
pub fn cues(vault_root: &Path, project: &str, label: &str) -> Option<PathBuf> {
    let path = vault_root
        .join("Projects")
        .join(project)
        .join("lighting")
        .join("cues")
        .join(format!("{}.json", slug(label)));
    path.exists().then_some(path)
}

/// Writes a setlist file of `sessions` in order (one path a line, what
/// the engine's `read_setlist` takes) for setlist `title`: where it is.
///
/// # Errors
///
/// The file could not be written.
pub fn write_setlist(title: &str, sessions: &[PathBuf]) -> std::io::Result<PathBuf> {
    let dir = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("studio")
        .join("launcher");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.setlist", slug(title)));
    let mut text = format!("# {title}: written by the launcher from the vault's setlist.\n");
    for session in sessions {
        text.push_str(&session.display().to_string());
        text.push('\n');
    }
    std::fs::write(&path, text)?;
    Ok(path)
}

/// An instrument's saved rig profiles on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rigs {
    /// Signal's name for the instrument (`guitar`, `keys`).
    pub instrument: String,
    pub profiles: Vec<String>,
}

/// The rigs saved in `signal_dir` (Signal's config folder): every
/// instrument folder with `profiles/<name>.styx`.
#[must_use]
pub fn rigs_in(signal_dir: &Path) -> Vec<Rigs> {
    let mut out: Vec<Rigs> = std::fs::read_dir(signal_dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| {
            let mut profiles: Vec<String> = std::fs::read_dir(e.path().join("profiles"))
                .ok()?
                .filter_map(Result::ok)
                .map(|p| p.path())
                .filter(|p| p.extension().is_some_and(|x| x == "styx"))
                .filter_map(|p| p.file_stem()?.to_str().map(str::to_owned))
                .collect();
            profiles.sort();
            // Signal keeps the guitar's rig in `rig/` (`Instrument::dir_name`).
            let folder = e.file_name();
            let instrument = match folder.to_str()? {
                "rig" => "guitar",
                other => other,
            };
            Some(Rigs {
                instrument: instrument.to_owned(),
                profiles,
            })
        })
        .collect();
    out.sort_by(|a, b| a.instrument.cmp(&b.instrument));
    out
}

/// The rigs saved on this machine (`~/.config/signal/<instrument>/profiles`).
#[must_use]
pub fn rigs() -> Vec<Rigs> {
    dirs::config_dir().map_or_else(Vec::new, |dir| rigs_in(&dir.join("signal")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch folder under the system's temp dir, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("studio-launcher-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn file(&self, path: &str) -> PathBuf {
            let at = self.0.join(path);
            std::fs::create_dir_all(at.parent().unwrap()).unwrap();
            std::fs::write(&at, "").unwrap();
            at
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_project_root_is_beside_the_vault() {
        let root = project_root(Path::new("/files/org/Vault"), "Show");
        assert_eq!(root, Path::new("/files/org/Projects/Show"));
    }

    #[test]
    fn a_song_is_found_by_its_folder() {
        let s = Scratch::new("songs");
        let drive = s.file("songs/drive-to-la/Drive To LA.session");
        s.file("songs/drive-to-la/Drive To LA.session-wal");
        let intro = s.file("songs/cinematic-intro/Cinematic Intro.session");
        s.file("unbreakable/Unbreakable.backup.RPP");
        let rpp = s.file("unbreakable/Unbreakable.RPP");
        assert_eq!(song_session(&s.0, "Drive To LA"), Some(drive));
        assert_eq!(song_session(&s.0, "DTLA Cinematic Intro"), Some(intro));
        assert_eq!(song_session(&s.0, "Unbreakable"), Some(rpp));
        assert_eq!(song_session(&s.0, "Sleeptalker"), None);
        assert_eq!(song_session(&s.0, "LA"), None);
    }

    #[test]
    fn a_written_session_is_looked_for_in_the_project_then_the_vault() {
        let s = Scratch::new("resolve");
        let in_project = s.file("Projects/Show/songs/a/A.session");
        let in_vault = s.file("Vault/Libraries/L/B/B.RPP");
        let (vault, project) = (s.0.join("Vault"), s.0.join("Projects/Show"));
        assert_eq!(
            resolve("songs/a/A.session", &vault, &project),
            Some(in_project.clone())
        );
        assert_eq!(
            resolve("Libraries/L/B/B.RPP", &vault, &project),
            Some(in_vault)
        );
        assert_eq!(
            resolve(in_project.to_str().unwrap(), &vault, &project),
            Some(in_project)
        );
        assert_eq!(
            resolve("../Vault/Libraries/L/B/B.RPP", &vault, &project),
            None
        );
        assert_eq!(resolve("songs/none.session", &vault, &project), None);
    }

    #[test]
    fn cues_are_named_for_the_place_in_the_set() {
        let s = Scratch::new("cues");
        let cue = s.file("Projects/Show/lighting/cues/know-me-extended.json");
        assert_eq!(cues(&s.0, "Show", "Know Me (Extended)"), Some(cue));
        assert_eq!(cues(&s.0, "Show", "Know Me"), None);
    }

    #[test]
    fn rigs_are_each_instruments_profiles() {
        let s = Scratch::new("rigs");
        s.file("rig/profiles/worship.styx");
        s.file("rig/profiles/clean.styx");
        s.file("rig/songs.styx");
        s.file("keys/profiles/stage.styx");
        s.file("notes.txt");
        let rigs = rigs_in(&s.0);
        assert_eq!(rigs.len(), 2);
        assert_eq!(rigs[0].instrument, "guitar");
        assert_eq!(rigs[0].profiles, ["clean", "worship"]);
        assert_eq!(rigs[1].profiles, ["stage"]);
    }
}
