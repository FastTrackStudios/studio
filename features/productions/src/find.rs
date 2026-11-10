//! Finding productions on disk: a folder walked for DAW project files and
//! project folders.
//!
//! - **Ableton**: a folder with `.als` files and an `Ableton Project
//!   Info` folder (Live makes one in every project), or named `… Project`,
//!   is ONE project: its sets are its files, the newest saved the one it
//!   opens. An `.als` anywhere else is a project of its own.
//! - **Pro Tools**: a folder with a session (`.ptx`, `.ptf`, `.pts`) and
//!   `Audio Files`, `Session File Backups`, `Clip Groups`, `Video Files`
//!   or `Rendered Files`, or whose session has the folder's name, is ONE
//!   project. A session anywhere else is a project of its own.
//! - **REAPER**: each `.rpp` is a project (REAPER marks no project
//!   folder; `.rpp-bak` is not read).
//! - **Logic**: a `.logicx` bundle is a project.
//!
//! A project folder is not walked further, and `Backup`, `Backups` and
//! `Session File Backups` never are: what is in them are older saves.
//! Hidden folders and symlinked folders are skipped. Nothing is written.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::model::Daw;

/// A production found on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub daw: Daw,
    /// Its name: the project folder's (Live's ` Project` left off), or
    /// the lone file's without its extension.
    pub name: String,
    /// The project folder, or the lone file (a `.logicx` bundle): what a
    /// re-scan knows it by.
    pub location: PathBuf,
    /// The file its DAW opens.
    pub file: PathBuf,
    /// A project folder's DAW files, relative to it; a lone file's name.
    pub files: Vec<String>,
    /// When `file` was last saved (its modification time).
    pub saved: Option<SystemTime>,
}

const MAX_DEPTH: usize = 24;
const BACKUPS: [&str; 3] = ["Backup", "Backups", "Session File Backups"];
const ABLETON_INFO: &str = "Ableton Project Info";
const ABLETON_SUFFIX: &str = " Project";
const PRO_TOOLS_MARKS: [&str; 5] = [
    "Audio Files",
    "Session File Backups",
    "Clip Groups",
    "Video Files",
    "Rendered Files",
];

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn saved(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// The DAW whose project file `path` is, by its extension.
#[must_use]
pub fn daw_of_file(path: &Path) -> Option<Daw> {
    match extension(path).as_str() {
        "als" => Some(Daw::Ableton),
        "ptx" | "ptf" | "pts" => Some(Daw::ProTools),
        "rpp" => Some(Daw::Reaper),
        _ => None,
    }
}

fn is_logic_bundle(path: &Path) -> bool {
    extension(path) == "logicx" && path.is_dir()
}

/// A `.logicx` bundle: saved when its newest alternative was.
fn bundle(path: &Path) -> Found {
    let newest = std::fs::read_dir(path.join("Alternatives"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| saved(&e.path().join("ProjectData")))
        .max();
    Found {
        daw: Daw::Logic,
        name: stem_of(path),
        location: path.to_path_buf(),
        file: path.to_path_buf(),
        files: vec![name_of(path)],
        saved: newest.or_else(|| saved(path)),
    }
}

/// A project file that is a project of its own.
fn lone(daw: Daw, path: &Path) -> Found {
    Found {
        daw,
        name: stem_of(path),
        location: path.to_path_buf(),
        file: path.to_path_buf(),
        files: vec![name_of(path)],
        saved: saved(path),
    }
}

/// A folder that is one project: `named` the file it opens, else the
/// newest saved.
fn folder(daw: Daw, dir: &Path, name: &str, files: &[&PathBuf], named: Option<&PathBuf>) -> Found {
    let main = named
        .or_else(|| files.iter().copied().max_by_key(|f| saved(f)))
        .cloned()
        .unwrap_or_else(|| dir.to_path_buf());
    Found {
        daw,
        name: name.to_owned(),
        location: dir.to_path_buf(),
        saved: saved(&main),
        file: main,
        files: files.iter().map(|f| name_of(f)).collect(),
    }
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<Found>) {
    if depth > MAX_DEPTH {
        return;
    }
    if is_logic_bundle(dir) {
        out.push(bundle(dir));
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(Daw, PathBuf)> = Vec::new();
    let mut dirs: Vec<(String, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            dirs.push((name, path));
        } else if path.is_file()
            && let Some(daw) = daw_of_file(&path)
        {
            files.push((daw, path));
        }
    }
    files.sort();
    dirs.sort();
    let has_dir = |name: &str| dirs.iter().any(|(d, _)| d == name);
    let of = |daw: Daw| -> Vec<&PathBuf> {
        files
            .iter()
            .filter(|(d, _)| *d == daw)
            .map(|(_, p)| p)
            .collect()
    };
    let dir_name = name_of(dir);
    let mut whole = false;

    let sets = of(Daw::Ableton);
    if !sets.is_empty() {
        if has_dir(ABLETON_INFO) || dir_name.ends_with(ABLETON_SUFFIX) {
            let name = dir_name.strip_suffix(ABLETON_SUFFIX).unwrap_or(&dir_name);
            out.push(folder(Daw::Ableton, dir, name, &sets, None));
            whole = true;
        } else {
            out.extend(sets.iter().map(|s| lone(Daw::Ableton, s)));
        }
    }

    let sessions = of(Daw::ProTools);
    if !sessions.is_empty() {
        let named = sessions.iter().copied().find(|s| stem_of(s) == dir_name);
        if named.is_some() || PRO_TOOLS_MARKS.iter().any(|m| has_dir(m)) {
            out.push(folder(Daw::ProTools, dir, &dir_name, &sessions, named));
            whole = true;
        } else {
            out.extend(sessions.iter().map(|s| lone(Daw::ProTools, s)));
        }
    }

    out.extend(of(Daw::Reaper).iter().map(|p| lone(Daw::Reaper, p)));

    if whole {
        return;
    }
    for (name, path) in &dirs {
        if !BACKUPS.contains(&name.as_str()) {
            walk(path, depth.saturating_add(1), out);
        }
    }
}

/// Every production under `path` (a folder, or one project file), by
/// where it is.
#[must_use]
pub fn find(path: &Path) -> Vec<Found> {
    let mut out = Vec::new();
    if path.is_file() {
        if let Some(daw) = daw_of_file(path) {
            out.push(lone(daw, path));
        }
    } else {
        walk(path, 0, &mut out);
    }
    out.sort_by(|a, b| a.location.cmp(&b.location).then(a.daw.cmp(&b.daw)));
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn a_project_folder_is_one_project() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        touch(&root.join("Album Project/Album.als"));
        touch(&root.join("Album Project/Album v2.als"));
        touch(&root.join("Album Project/Backup/Album [2026-01-01 101010].als"));
        std::fs::create_dir_all(root.join("Album Project/Ableton Project Info")).unwrap();
        touch(&root.join("Mix/Mix.ptx"));
        touch(&root.join("Mix/Mix.bak.ptx"));
        touch(&root.join("Mix/Session File Backups/Mix.bak.001.ptx"));
        std::fs::create_dir_all(root.join("Mix/Audio Files")).unwrap();
        touch(&root.join("Loose/One.als"));
        touch(&root.join("Loose/Two.als"));
        touch(&root.join("Loose/Backup/One [old].als"));
        touch(&root.join("Loose/Take.rpp"));
        touch(&root.join("Loose/Take.rpp-bak"));
        touch(&root.join("Song.logicx/Alternatives/000/ProjectData"));
        touch(&root.join(".hidden/Secret.als"));
        touch(&root.join("notes.md"));

        let found = find(root);
        let got: Vec<(Daw, &str, usize)> = found
            .iter()
            .map(|f| (f.daw, f.name.as_str(), f.files.len()))
            .collect();
        assert_eq!(
            got,
            [
                (Daw::Ableton, "Album", 2),
                (Daw::Ableton, "One", 1),
                (Daw::Reaper, "Take", 1),
                (Daw::Ableton, "Two", 1),
                (Daw::ProTools, "Mix", 2),
                (Daw::Logic, "Song", 1),
            ]
        );
        let mix = found.iter().find(|f| f.name == "Mix").unwrap();
        assert_eq!(mix.file, root.join("Mix/Mix.ptx"));
        assert_eq!(mix.location, root.join("Mix"));
        // The folder itself, or one file of it, scans the same way.
        assert_eq!(find(&root.join("Album Project")).len(), 1);
        assert_eq!(find(&root.join("Loose/One.als")).len(), 1);
        assert_eq!(find(&root.join("notes.md")), []);
    }
}
