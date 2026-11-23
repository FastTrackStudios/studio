//! Setlists that are files in Drive (`<Set>.setlist`, beside the songs'
//! session folders): found in an org's indexes, read, and each song tied
//! to its session folder and its chart (`.kf`, wherever Drive has one).
//! The file stays the set; Studio reads it each time it is opened.
//!
//! Natively, Studio's own folder (`storage::setlists`) has setlist files
//! too, read from the disk ([`on_disk`]), with or without Drive.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::data::{self, Song};
use crate::files::{self, Hit};
use crate::files_live::{disk, web};
use crate::setfile;
use crate::state::Place;

thread_local! {
    /// Every chart each space's indexes have, asked once a page.
    static CHARTS: RefCell<HashMap<String, Vec<Hit>>> = RefCell::new(HashMap::new());
    /// Each setlist file as read, so opening it again is instant.
    static READ: RefCell<HashMap<String, FileSet>> = RefCell::new(HashMap::new());
}

/// The setlist files in one space's indexes, by name.
pub async fn files(space: &str) -> Vec<Hit> {
    let span = crate::trace::Span::root("studio.setlists.find");
    let mut v = web::search_in(Some(&span), space, "", &["setlist".to_string()], 200)
        .await
        .into_iter()
        .filter(|h| h.name.ends_with(".setlist") && !h.dir)
        .collect::<Vec<_>>();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v.dedup_by(|a, b| a.location == b.location && a.path == b.path);
    v
}

/// The setlist files in Studio's own folder (`<home>/Setlists`), by name.
pub fn on_disk(space: &str) -> Vec<Hit> {
    let mut v: Vec<Hit> = disk::list(space, "Setlists")
        .into_iter()
        .filter(|h| h.name.ends_with(".setlist") && !h.dir && !is_studios(h))
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

/// A file Studio writes for one of its own sets (`crate::set_files`):
/// that set is listed as itself, not again as a file.
fn is_studios(h: &Hit) -> bool {
    disk::bytes(&disk::url(&h.path))
        .and_then(Result::ok)
        .is_some_and(|(_, b)| crate::set_files::set_id(&String::from_utf8_lossy(&b)).is_some())
}

/// The files on disk first, then Drive's, each once: a Drive file with
/// the same path, or the same name as one on disk, is that one.
pub fn merge(on_disk: &[Hit], from_drive: &[Hit]) -> Vec<Hit> {
    let mut out: Vec<Hit> = on_disk.to_vec();
    for h in from_drive {
        if !out.iter().any(|o| {
            o.path == h.path || o.path.ends_with(&format!("/{}", h.path)) || o.name == h.name
        }) {
            out.push(h.clone());
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The setlist files in every one of these spaces, each once.
pub async fn files_in(spaces: &[String]) -> Vec<Hit> {
    let found = futures_util::future::join_all(spaces.iter().map(|s| files(s))).await;
    let mut out: Vec<Hit> = Vec::new();
    for h in found.into_iter().flatten() {
        if !out
            .iter()
            .any(|o| o.location == h.location && o.path == h.path)
        {
            out.push(h);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Every chart in these spaces.
pub async fn charts(parent: Option<&crate::trace::Span>, spaces: &[String]) -> Vec<Hit> {
    // Every space at once (each search can take a while).
    let found = futures_util::future::join_all(spaces.iter().map(|s| async move {
        if let Some(h) = CHARTS.with(|c| c.borrow().get(s).cloned()) {
            return h;
        }
        let hits = web::search_in(parent, s, "", &["kf".to_string()], 1000).await;
        // None found (or no node): asked again next time.
        if !hits.is_empty() {
            CHARTS.with(|c| c.borrow_mut().insert(s.clone(), hits.clone()));
        }
        hits
    }))
    .await;
    found.into_iter().flatten().collect()
}

/// What a file on disk's set page watches: the file's time and how many
/// charts Drive has found so far. Read again when either moves.
pub fn disk_version(path: &str, spaces: &[String]) -> (i64, usize) {
    let mtime = disk::list("", path.rsplit_once('/').map_or("", |(d, _)| d))
        .into_iter()
        .find(|h| h.path == path)
        .map_or(0, |h| h.mtime);
    (mtime, cached_charts(spaces).len())
}

/// Where a song line's session folder is: (space, location, folder).
/// An id is looked for on this computer's disk (a setlist from Studio's
/// folder or an external drive, `song_ids`), then in Drive's index; an
/// older path line is relative to the setlist where it is.
async fn locate(
    span: &crate::trace::Span,
    space: &str,
    location: &str,
    path: &str,
    line: &str,
    spaces: &[String],
) -> Option<(String, String, String)> {
    let here = |f: String| Some((space.to_string(), location.to_string(), f));
    match setfile::song(line) {
        setfile::Song::Title(_) => None,
        setfile::Song::Path(p) => {
            // On disk an older line resolves against the file's real path.
            if location == files::DISK {
                let file = crate::storage::resolve(path)?;
                let f = crate::song_ids::folder_of(p, &file)?;
                return here(f.to_string_lossy().into_owned());
            }
            here(setfile::folder(path, p))
        }
        setfile::Song::Id { id, hint } => {
            if location == files::DISK
                && let Some(f) = crate::storage::resolve(path)
                    .and_then(|file| crate::song_ids::folder_of(line, &file))
            {
                return here(f.to_string_lossy().into_owned());
            }
            in_drive(span, id, hint, spaces).await
        }
    }
}

/// A session with this id in Drive's index: its sessions named like the
/// hint, each one's header read.
async fn in_drive(
    span: &crate::trace::Span,
    id: &str,
    hint: &str,
    spaces: &[String],
) -> Option<(String, String, String)> {
    if hint.is_empty() {
        return None;
    }
    for s in spaces {
        let hits = web::search_in(
            Some(span),
            s,
            hint,
            &["session".to_string(), "fts-session".to_string()],
            20,
        )
        .await;
        for h in hits.iter().filter(|h| !h.dir) {
            let Ok((200 | 206, b)) =
                web::bytes_in(Some(span), "GET", &web::file_url(h), None).await
            else {
                continue;
            };
            if crate::song_ids::id(&b).as_deref() == Some(id) {
                let folder = h.path.rsplit_once('/').map_or("", |(d, _)| d).to_string();
                return Some((h.space.clone(), h.location.clone(), folder));
            }
        }
    }
    None
}

/// The charts already found in these spaces.
fn cached_charts(spaces: &[String]) -> Vec<Hit> {
    CHARTS.with(|c| {
        let c = c.borrow();
        spaces
            .iter()
            .filter_map(|s| c.get(s))
            .flatten()
            .cloned()
            .collect()
    })
}

/// A song of a setlist file.
#[derive(Clone, Debug, PartialEq)]
pub struct FileSong {
    pub title: String,
    pub place: Place,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileAct {
    pub name: Option<String>,
    pub songs: Vec<FileSong>,
}

/// A setlist file, read.
#[derive(Clone, Debug, PartialEq)]
pub struct FileSet {
    pub name: String,
    pub location_name: String,
    pub acts: Vec<FileAct>,
}

impl FileSet {
    /// Its songs in order, ready to play.
    pub fn playlist(&self) -> Vec<(Song, Place)> {
        self.acts
            .iter()
            .flat_map(|a| a.songs.iter())
            .map(|s| (data::song_named(&s.title), s.place.clone()))
            .collect()
    }
}

/// Read the setlist file at `path` in `location` of `space`; `spaces` are
/// where its songs' charts may be.
pub async fn load(
    space: &str,
    location: &str,
    path: &str,
    spaces: &[String],
) -> Result<FileSet, String> {
    // A file on disk is read again each time (it may have been edited).
    let key = format!("{space}/{location}/{path}");
    if location != files::DISK
        && let Some(set) = READ.with(|r| r.borrow().get(&key).cloned())
    {
        return Ok(set);
    }
    let mut span = crate::trace::Span::root("studio.setlist.open");
    span.set("studio.setlist", path);
    let set = read(&span, space, location, path, spaces).await;
    match &set {
        Ok(s) => {
            span.set(
                "studio.songs",
                s.acts.iter().map(|a| a.songs.len()).sum::<usize>() as u64,
            );
            READ.with(|r| r.borrow_mut().insert(key, s.clone()));
        }
        Err(e) => span.fail(e.clone()),
    }
    set
}

async fn read(
    span: &crate::trace::Span,
    space: &str,
    location: &str,
    path: &str,
    spaces: &[String],
) -> Result<FileSet, String> {
    let name = path.rsplit('/').next().unwrap_or(path).to_string();
    let file = Hit {
        space: space.into(),
        location: location.into(),
        path: path.into(),
        name: name.clone(),
        online: true,
        ..Default::default()
    };
    // 503: the computer that has it is (re)connecting to the node; a few
    // seconds' grace before saying so.
    let mut tries = 0;
    let text = loop {
        match web::bytes_in(Some(span), "GET", &web::file_url(&file), None).await? {
            (200 | 206, b) => break String::from_utf8_lossy(&b).into_owned(),
            (503, _) if tries < 10 => {
                tries += 1;
                crate::state::sleep_ms(1000).await;
            }
            (code, _) => return Err(format!("its computer answered {code}")),
        }
    };
    let parsed = setfile::parse(&text);
    let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
    // Each song's place: found by its id (on disk, then in Drive), or by
    // an older line's path; `= Title` has none.
    let lines: Vec<(usize, &str)> = parsed
        .acts
        .iter()
        .enumerate()
        .flat_map(|(a, act)| act.songs.iter().map(move |l| (a, l.as_str())))
        .collect();
    let places = futures_util::future::join_all(
        lines
            .iter()
            .map(|(_, l)| locate(span, space, location, path, l, spaces)),
    )
    .await;
    // The charts, the file's folder and every song's folder, at once.
    let (charts, siblings, listed) = futures_util::join!(
        async {
            // A song's own folder's chart comes first (below), then Drive's.
            // A file on disk opens without waiting on Drive's search: the
            // charts it already found, and a search for next time.
            if location == files::DISK {
                let warm = spaces.to_vec();
                dioxus::prelude::spawn(async move {
                    charts(None, &warm).await;
                });
                cached_charts(spaces)
            } else {
                charts(Some(span), spaces).await
            }
        },
        web::list_in(Some(span), space, location, dir),
        futures_util::future::join_all(places.iter().map(|p| async move {
            match p {
                Some((s, l, f)) => web::list_in(Some(span), s, l, f).await,
                None => Vec::new(),
            }
        })),
    );
    let location_name = siblings
        .first()
        .map(|h| h.location_name.clone())
        .unwrap_or_default();
    let mut acts: Vec<FileAct> = parsed
        .acts
        .iter()
        .map(|a| FileAct {
            name: a.name.clone(),
            songs: Vec::new(),
        })
        .collect();
    for (((a, line), place), hits) in lines.into_iter().zip(places).zip(listed) {
        let hint = match setfile::song(line) {
            setfile::Song::Id { hint, .. } | setfile::Song::Title(hint) => hint.to_string(),
            setfile::Song::Path(p) => setfile::title_of(&setfile::folder(path, p), &[]),
        };
        let Some((s, l, folder)) = place else {
            // No session found (or none yet): the song by its title.
            acts[a].songs.push(FileSong {
                place: Place {
                    space: space.into(),
                    location: location.into(),
                    chart: crate::reference::pick_chart(&hint, &charts).cloned(),
                    folder: String::new(),
                },
                title: hint,
            });
            continue;
        };
        // Its own chart first (a `.kf` in its folder), then one named like it.
        let own = hits
            .iter()
            .find(|h| !h.dir && h.name.ends_with(".kf"))
            .cloned();
        let names: Vec<String> = hits.into_iter().map(|h| h.name).collect();
        let title = if names.is_empty() && !hint.is_empty() {
            hint
        } else {
            setfile::title_of(&folder, &names)
        };
        acts[a].songs.push(FileSong {
            place: Place {
                chart: own
                    .or_else(|| crate::reference::pick_chart(&folder, &charts).cloned())
                    .or_else(|| crate::reference::pick_chart(&title, &charts).cloned()),
                space: s,
                location: l,
                folder,
            },
            title,
        });
    }
    Ok(FileSet {
        name: setfile::name_of(&name),
        location_name,
        acts,
    })
}
