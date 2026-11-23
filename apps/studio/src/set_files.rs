//! Studio's sets as `.setlist` files in its folder (`storage::setlists`),
//! both ways, natively: a set made or edited in the app is written to
//! `<home>/Setlists/<title>.setlist`, and an edit made to that file by
//! hand is read back into the set. The file's first line says which set
//! it is (`# Studio set <id>`); its songs are the set's, a song line its
//! session folder where the file had one, else `= Title`.
//!
//! Every two seconds: a set whose document changed since it was last
//! written is written again; a file that changed since it was last read
//! is read into its set. Nothing is ever deleted: a set renamed leaves
//! its old file only when that file is exactly what Studio wrote. A file
//! deleted by hand takes its set out of the org's list (the set's
//! document is kept); Studio never writes it back.

// A browser has no disk: what reads and writes it is the native build's.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use loro::LoroDoc;

use crate::setfile::{self, Line};
use crate::setlist::{self, Entry, ItemKind};
use crate::state::use_app;

const HEAD: &str = "Studio set ";

/// Which of Studio's sets a setlist file is (its `# Studio set <id>`
/// comment, among the comments it starts with).
pub fn set_id(text: &str) -> Option<&str> {
    text.lines()
        .map(str::trim)
        .take_while(|l| l.starts_with('#'))
        .find_map(|l| {
            l.trim_start_matches('#')
                .trim()
                .strip_prefix(HEAD)
                .map(|r| r.split_whitespace().next().unwrap_or(""))
        })
        .filter(|id| !id.is_empty())
}

/// A new file's own id (a random UUID, version 4).
fn new_file_id() -> String {
    let h = drive_dioxus::env::random_hex32();
    let h = if h.len() >= 32 {
        h
    } else {
        format!("{h:0<32}")
    };
    // Version 4, variant 10xx.
    let variant = ['8', '9', 'a', 'b'][usize::from(h.as_bytes()[16] % 4)];
    format!(
        "{}-{}-4{}-{variant}{}-{}",
        &h[0..8],
        &h[8..12],
        &h[13..16],
        &h[17..20],
        &h[20..32]
    )
}

/// What Studio last wrote and last read for a set.
#[derive(Default)]
struct Seen {
    file: PathBuf,
    /// The text the set's document gave when last written or read.
    made: String,
    /// The file's text when last written or read.
    text: String,
}

thread_local! {
    static SEEN: RefCell<HashMap<String, Seen>> = RefCell::new(HashMap::new());
}

/// Keeps the org's sets and their files in step, while the app runs.
#[component]
pub fn Sync() -> Element {
    let app = use_app();
    let index = crate::sets::use_index();
    let drive = try_use_context::<drive_dioxus::DriveCtx>();
    use_future(move || {
        let (index, drive) = (index.clone(), drive.clone());
        async move {
            let (Some(drive), Some(dir)) = (drive, crate::storage::setlists()) else {
                return;
            };
            loop {
                let org = app.here.peek().clone();
                for f in index.faces() {
                    let id = crate::orgs::qualified(&org, &setlist::setlist_doc(&f.id));
                    let full = drive.qualify(&id);
                    let doc = drive.drive.open(&full);
                    if setlist::info(&doc).title.is_empty() {
                        continue; // not read from this device's copy yet
                    }
                    match step(&dir, &f.id, &doc) {
                        Step::Same => {}
                        Step::Read => {
                            drive.drive.commit(&full);
                            index.put_face(&setlist::face_of(&doc));
                        }
                        Step::Deleted => {
                            let when = format!("{:.0}", drive_dioxus::env::now_ms());
                            tracing::info!(studio.set = %f.id, "set file deleted by hand: set taken out of the list");
                            index.remove(&f.id, &when);
                        }
                    }
                }
                crate::state::sleep_ms(2000).await;
            }
        }
    });
    rsx! {}
}

/// The playing song's sections, natively: read from its session file
/// (`<folder>/<Title>.session`) when its folder is on this computer.
#[component]
pub fn Sections() -> Element {
    let app = use_app();
    let np = (app.np)();
    let key = np.as_ref().and_then(|n| {
        let p = n.place()?;
        (p.location == crate::files::DISK && !p.folder.is_empty()).then(|| {
            (
                n.i,
                p.folder.clone(),
                n.song().and_then(|s| s.tempo).unwrap_or(120),
            )
        })
    });
    use_effect(use_reactive!(|key| {
        let Some((i, folder, bpm)) = key else { return };
        let t = session_of(&folder, f64::from(bpm)).unwrap_or_default();
        let mut np = app.np;
        np.with_mut(|n| {
            if let Some(n) = n.as_mut().filter(|n| n.i == i) {
                if let Some(end) = t.sections.last().map(|s| s.end) {
                    n.dur = end;
                }
                n.timeline = t;
            }
        });
    }));
    rsx! {}
}

fn session_of(folder: &str, bpm: f64) -> Option<crate::session_file::Timeline> {
    let dir = crate::storage::resolve(folder)?;
    // The first session file that reads (`<Title>.session`, or an older
    // `.fts-session` beside an engine `.session`).
    crate::song_ids::sessions(&dir)
        .find_map(|f| crate::session_file::read(&std::fs::read(f).ok()?, bpm).ok())
}

/// One set: write it, or read its file into it. True when the set changed.
/// What one pass did with a set and its file.
#[derive(Debug, PartialEq)]
enum Step {
    /// Nothing, or the set written to its file.
    Same,
    /// The file's edits read into the set.
    Read,
    /// The file Studio wrote or read is gone: deleted by hand.
    Deleted,
}

fn step(dir: &Path, id: &str, doc: &LoroDoc) -> Step {
    let info = setlist::info(doc);
    let found = find(dir, id);
    let file = found
        .clone()
        .unwrap_or_else(|| dir.join(file_name(&info.title)));
    let on_disk = std::fs::read_to_string(&file).ok();
    SEEN.with(|seen| {
        let mut seen = seen.borrow_mut();
        let first = !seen.contains_key(id);
        // A file this run has had, gone from the folder: deleted by hand
        // (only when the folder itself is there, so an unmounted Drive
        // never reads as every set deleted).
        if !first && found.is_none() && on_disk.is_none() && dir.is_dir() {
            seen.remove(id);
            return Step::Deleted;
        }
        let s = seen.entry(id.to_string()).or_default();
        // Song lines the file has, by title, kept when the set is written.
        let lines = on_disk
            .as_deref()
            .map(|t| lines_of(&file, t))
            .unwrap_or_default();
        // The file keeps its own id; a new file gets one.
        let file_id = on_disk
            .as_deref()
            .and_then(|t| setfile::parse(t).id)
            .unwrap_or_else(new_file_id);
        let made = text(&file_id, id, doc, &lines);
        match on_disk {
            // Edited by hand (or there at the start): the file wins.
            Some(t) if (first || t != s.text) && t != made => {
                let changed = read_into(doc, &file, &t);
                *s = Seen {
                    made: text(&file_id, id, doc, &lines_of(&file, &t)),
                    text: t,
                    file,
                };
                if changed { Step::Read } else { Step::Same }
            }
            Some(t) if !first && made == s.made && file == s.file => {
                s.text = t;
                Step::Same
            }
            _ => {
                // The set changed (or has no file yet): write it, under its
                // title; a rename moves the file Studio wrote.
                let want = dir.join(file_name(&info.title));
                if want != file && std::fs::read_to_string(&file).ok() == Some(s.text.clone()) {
                    let _ = std::fs::rename(&file, &want);
                }
                // The Setlists folder is made in a home that is there, never
                // the home itself: a Drive mount that has dropped out leaves
                // an empty mount point, and nothing may be written into it.
                if !dir.is_dir() && !dir.parent().is_some_and(Path::is_dir) {
                    return Step::Same;
                }
                let _ = std::fs::create_dir(dir);
                if write(&want, &made) {
                    *s = Seen {
                        file: want,
                        text: made.clone(),
                        made,
                    };
                }
                Step::Same
            }
        }
    })
}

fn write(path: &Path, text: &str) -> bool {
    let tmp = path.with_extension("setlist.tmp");
    std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, path).is_ok()
}

/// The setlist file in `dir` that says it is set `id`, if there is one.
pub fn file_of(dir: &Path, id: &str) -> Option<PathBuf> {
    find(dir, id)
}

/// The file in `dir` that says it is set `id`.
fn find(dir: &Path, id: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.extension().is_some_and(|x| x == "setlist")
                && std::fs::read_to_string(p)
                    .ok()
                    .is_some_and(|t| set_id(&t) == Some(id))
        })
}

/// A set's title as a file name: what a file system refuses, out.
pub fn file_name(title: &str) -> String {
    let t: String = title
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '\0') {
                '-'
            } else {
                c
            }
        })
        .collect();
    let t = t.trim().trim_start_matches('.');
    format!("{}.setlist", if t.is_empty() { "Untitled" } else { t })
}

/// A file's song lines by title (folder lines titled as a set page does).
fn lines_of(file: &Path, text: &str) -> HashMap<String, String> {
    let path = file.to_string_lossy();
    setfile::parse(text)
        .songs()
        .into_iter()
        .map(|l| (title_of_line(&path, l), l.to_string()))
        .collect()
}

fn title_of_line(file: &str, line: &str) -> String {
    let folder = match setfile::song(line) {
        setfile::Song::Title(t) => return t.to_string(),
        setfile::Song::Id { hint, .. } => match crate::song_ids::folder_of(line, Path::new(file)) {
            Some(f) => f.to_string_lossy().into_owned(),
            None => return hint.to_string(),
        },
        setfile::Song::Path(p) => setfile::folder(file, p),
    };
    let names: Vec<String> = std::fs::read_dir(&folder)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    setfile::title_of(&folder, &names)
}

/// The set as a file's text: its sections as acts, its songs.
fn text(file_id: &str, id: &str, doc: &LoroDoc, lines: &HashMap<String, String>) -> String {
    let mut acts: Vec<(Option<String>, Vec<Line>)> = vec![(None, Vec::new())];
    for e in setlist::entries(doc) {
        match e.kind {
            ItemKind::Section => acts.push((Some(e.title.clone()), Vec::new())),
            ItemKind::Song => {
                // The line the file had for it; else its session's id when
                // the home's Songs/ has one by its title; else its title.
                let line = match lines.get(&e.title).map(|l| (l, setfile::song(l))) {
                    Some((_, setfile::Song::Id { id, .. })) => Line::Id {
                        id: id.to_string(),
                        title: e.title.clone(),
                    },
                    Some((l, setfile::Song::Path(_))) => Line::Kept(l.clone()),
                    _ => match crate::song_ids::by_title(&e.title) {
                        Some(id) => Line::Id {
                            id,
                            title: e.title.clone(),
                        },
                        None => Line::Title(e.title.clone()),
                    },
                };
                if let Some(a) = acts.last_mut() {
                    a.1.push(line);
                }
            }
            _ => {}
        }
    }
    acts.retain(|(n, s)| n.is_some() || !s.is_empty());
    let title = setlist::info(doc).title;
    setfile::write(file_id, &format!("{HEAD}{id} · {title}"), &acts)
}

/// Read a file into its set: its acts and songs replace the set's
/// sections and songs (a song already in the set keeps its key and
/// notes). True when anything changed.
fn read_into(doc: &LoroDoc, file: &Path, text: &str) -> bool {
    let path = file.to_string_lossy();
    let parsed = setfile::parse(text);
    let mut want: Vec<(ItemKind, String)> = Vec::new();
    for a in &parsed.acts {
        if let Some(n) = &a.name {
            want.push((ItemKind::Section, n.clone()));
        }
        for l in &a.songs {
            want.push((ItemKind::Song, title_of_line(&path, l)));
        }
    }
    let had: Vec<Entry> = setlist::entries(doc)
        .into_iter()
        .filter(|e| matches!(e.kind, ItemKind::Song | ItemKind::Section))
        .collect();
    let now: Vec<(ItemKind, String)> = had.iter().map(|e| (e.kind, e.title.clone())).collect();
    if now == want {
        return false;
    }
    for e in &had {
        let _ = setlist::remove_entry(doc, &e.id);
    }
    let mut spare = had;
    for (kind, title) in want {
        let e = match spare
            .iter()
            .position(|e| e.kind == kind && e.title.eq_ignore_ascii_case(&title))
        {
            Some(i) => {
                let mut e = spare.remove(i);
                e.id.clear();
                e
            }
            None => Entry {
                kind,
                title,
                ..Default::default()
            },
        };
        let _ = setlist::insert_entry(doc, None, &e);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_says_which_set_it_is() {
        assert_eq!(set_id("# Studio set s12 · Sunday\n\n= A\n"), Some("s12"));
        assert_eq!(
            set_id("# id: 45b794d1-d6ba-4d66-ab2d-6be35df2f6bc\n# Studio set s3 · X\n"),
            Some("s3")
        );
        assert_eq!(set_id("# JT's Senior Recital\nknow-me\n"), None);
        assert_eq!(file_name("Sunday 11/22: AM"), "Sunday 11-22- AM.setlist");
        assert_eq!(file_name("  "), "Untitled.setlist");
    }

    #[test]
    fn a_file_deleted_by_hand_takes_its_set_out() {
        let dir = std::env::temp_dir().join(format!("studio-set-files-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let doc = LoroDoc::new();
        let mut info = setlist::info(&doc);
        info.id = "sdel".into();
        info.title = "Deleted By Hand".into();
        setlist::put_info(&doc, &info).unwrap();
        // First pass writes the file.
        assert_eq!(step(&dir, "sdel", &doc), Step::Same);
        let file = dir.join(file_name(&info.title));
        assert!(file.exists());
        // Deleted by hand: the set is reported gone, not written back.
        std::fs::remove_file(&file).unwrap();
        assert_eq!(step(&dir, "sdel", &doc), Step::Deleted);
        assert!(!file.exists());

        // A folder that is gone (an unmounted Drive) is not every set deleted.
        assert_eq!(step(&dir, "sdel", &doc), Step::Same);
        std::fs::remove_dir_all(&dir).unwrap();
        let gone = dir.join("Setlists");
        assert_eq!(step(&gone, "sdel", &doc), Step::Same);
        assert!(!dir.exists(), "nothing is made where the home was");

        let index = LoroDoc::new();
        let face = setlist::face_of(&doc);
        setlist::put_face(&index, &face);
        assert_eq!(setlist::faces(&index).len(), 1);
        setlist::remove_set(&index, "sdel", "0");
        assert!(setlist::faces(&index).is_empty());
    }
}
