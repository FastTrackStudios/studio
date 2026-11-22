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
}

/// The setlist files in one space's indexes, by name.
pub async fn files(space: &str) -> Vec<Hit> {
    let mut v = web::search(space, "", &["setlist".to_string()], 200)
        .await
        .into_iter()
        .filter(|h| h.name.ends_with(".setlist") && !h.dir)
        .collect::<Vec<_>>();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v.dedup_by(|a, b| a.location == b.location && a.path == b.path);
    v
}

/// Every chart in these spaces.
pub async fn charts(spaces: &[String]) -> Vec<Hit> {
    let mut out = Vec::new();
    for s in spaces {
        if let Some(h) = CHARTS.with(|c| c.borrow().get(s).cloned()) {
            out.extend(h);
            continue;
        }
        let hits = web::search(s, "", &["kf".to_string()], 1000).await;
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
    let name = path.rsplit('/').next().unwrap_or(path).to_string();
    let file = Hit {
        space: space.into(),
        location: location.into(),
        path: path.into(),
        name: name.clone(),
        online: true,
        ..Default::default()
    };
    let text = match web::bytes("GET", &web::file_url(&file), None).await? {
        (200 | 206, b) => String::from_utf8_lossy(&b).into_owned(),
        (code, _) => return Err(format!("its computer answered {code}")),
    };
    let parsed = setfile::parse(&text);
    let charts = charts(spaces).await;
    let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
    let siblings = web::list(space, location, dir).await;
    let location_name = siblings
        .first()
        .map(|h| h.location_name.clone())
        .unwrap_or_default();
    let mut acts = Vec::new();
    for a in parsed.acts {
        let mut songs = Vec::new();
        for rel in &a.songs {
            let folder = setfile::folder(path, rel);
            let names: Vec<String> = web::list(space, location, &folder)
                .await
                .into_iter()
                .map(|h| h.name)
                .collect();
            songs.push(FileSong {
                title: setfile::title_of(&folder, &names),
                place: Place {
                    space: space.into(),
                    location: location.into(),
                    chart: crate::reference::pick_chart(&folder, &charts).cloned(),
                    folder,
                },
            });
        }
        acts.push(FileAct {
            name: a.name,
            songs,
        });
    }
    Ok(FileSet {
        name: setfile::name_of(&name),
        location_name,
        acts,
    })
}
