//! Setlists that are files in Drive (`<Set>.setlist`, beside the songs'
//! session folders): found in an org's indexes, read, and each song tied
//! to its session folder and its chart (`.kf`, wherever Drive has one).
//! The file stays the set; Studio reads it each time it is opened.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::data::{self, Song};
use crate::files::Hit;
use crate::files_live::web;
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
    let mut out = Vec::new();
    for s in spaces {
        if let Some(h) = CHARTS.with(|c| c.borrow().get(s).cloned()) {
            out.extend(h);
            continue;
        }
        let hits = web::search_in(parent, s, "", &["kf".to_string()], 1000).await;
        CHARTS.with(|c| c.borrow_mut().insert(s.clone(), hits.clone()));
        out.extend(hits);
    }
    out
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
    let key = format!("{space}/{location}/{path}");
    if let Some(set) = READ.with(|r| r.borrow().get(&key).cloned()) {
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
                gloo_timers::future::TimeoutFuture::new(1000).await;
            }
            (code, _) => return Err(format!("its computer answered {code}")),
        }
    };
    let parsed = setfile::parse(&text);
    let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
    // The charts, the file's folder and every song's folder, at once.
    let folders: Vec<(usize, String)> = parsed
        .acts
        .iter()
        .enumerate()
        .flat_map(|(a, act)| {
            act.songs
                .iter()
                .map(move |rel| (a, setfile::folder(path, rel)))
        })
        .collect();
    let (charts, siblings, listed) = futures_util::join!(
        charts(Some(span), spaces),
        web::list_in(Some(span), space, location, dir),
        futures_util::future::join_all(folders.iter().map(|(_, f)| web::list_in(
            Some(span),
            space,
            location,
            f
        ))),
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
    for ((a, folder), hits) in folders.into_iter().zip(listed) {
        let names: Vec<String> = hits.into_iter().map(|h| h.name).collect();
        acts[a].songs.push(FileSong {
            title: setfile::title_of(&folder, &names),
            place: Place {
                space: space.into(),
                location: location.into(),
                chart: crate::reference::pick_chart(&folder, &charts).cloned(),
                folder,
            },
        });
    }
    Ok(FileSet {
        name: setfile::name_of(&name),
        location_name,
        acts,
    })
}
