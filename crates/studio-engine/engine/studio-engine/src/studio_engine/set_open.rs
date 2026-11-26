//! The set's openers: a library setlist, a live set, a path, a new song,
//! and a song's colour by hand. They need what the core does not own (the
//! streamed set, Task's account, a new project, the window's opening of a
//! song), so they stay here, beside the core's set (`engine_core`'s
//! `studio_engine::set`), under the same path (`crate::studio_engine::set`).
//! They go with engine-project (`stream_set`, `project_new`).

use std::path::{Path, PathBuf};

use engine_core::studio_engine::set::{Busy, LibraryOpen, open_with, position_named, update, view};

use crate::open_progress::Progress;
use crate::setlist::{Setlist, SetlistOpen as _};

/// Give song `index` a colour by hand (`None`: the one its title makes) —
/// on its SONG region and in its `.session` (`Setlist::recolor`).
///
/// The region and the file are written outside the set's lock: in Remote
/// that is a call to the other system, which may take seconds.
pub fn recolor(index: usize, color: Option<String>) {
    let shown = view();
    let Some(mut setlist) = shown.setlist else {
        return;
    };
    setlist.recolor(index, color);
    let Some(recoloured) = setlist.songs.get(index).cloned() else {
        return;
    };
    update(|view| {
        let song = view
            .setlist
            .as_mut()
            .filter(|_| view.epoch == shown.epoch)
            .and_then(|s| s.songs.get_mut(index))
            .filter(|song| song.project == recoloured.project);
        let Some(song) = song else {
            return false;
        };
        song.color = recoloured.color;
        true
    });
}

/// Stream `request`'s setlist in from the library, as the person signed in
/// on this machine (`fts_account`, `FTS_ACCOUNT_FILE`): what a click on it
/// in a window does, and `SESSION_OPEN_SETLIST`.
///
/// # Errors
///
/// Another open is under way.
pub fn open_library(request: LibraryOpen) -> Result<(), Busy> {
    open_with(
        Progress::Joining { retry: None },
        Box::new(move |progress| open_library_now(&request, progress)),
    )
}

fn open_library_now(request: &LibraryOpen, progress: &dyn Fn(Progress)) -> eyre::Result<Setlist> {
    use crate::task_account::AccountLibrary as _;
    let account = crate::task_account::signed_in()
        .ok_or_else(|| eyre::eyre!("not signed in to Task on this machine"))?;
    let org = request
        .org
        .clone()
        .or_else(|| std::env::var("FTS_TASK_ORG").ok().filter(|o| !o.is_empty()))
        .or_else(crate::task_account::remembered_org)
        .ok_or_else(|| eyre::eyre!("which org's library? (`org`, or FTS_TASK_ORG)"))?;
    let library = account.library(&org);
    // One song of the library alone, in no list: a set of it.
    if request.setlist.trim().is_empty() {
        let want = request
            .song
            .clone()
            .ok_or_else(|| eyre::eyre!("which setlist, or which song?"))?;
        let songs = crate::stream_set::with_library(&library, |l| async move { l.songs().await })?;
        let song = songs
            .into_iter()
            .map(|(song, _)| song)
            .find(|s| s.slug.eq_ignore_ascii_case(&want) || s.title.eq_ignore_ascii_case(&want))
            .ok_or_else(|| eyre::eyre!("no song {want:?} in {org}'s library"))?;
        let setlist = crate::stream_set::LibrarySetlist {
            id: format!("song-{}", song.slug),
            title: song.title.clone(),
            kind: crate::stream_set::ListKind::Songs,
            songs: vec![song],
        };
        return crate::stream_set::open(
            crate::stream_set::Remote::Library {
                library,
                setlist,
                name: account.name(),
            },
            progress,
        );
    }
    let lists = crate::stream_set::setlists(&library)?;
    let titles: Vec<&str> = lists.iter().map(|l| l.title.as_str()).collect();
    let at = position_named(&titles, &request.setlist)
        .ok_or_else(|| eyre::eyre!("no setlist {:?} in {org}'s library", request.setlist))?;
    let mut setlist = lists
        .into_iter()
        .nth(at)
        .ok_or_else(|| eyre::eyre!("the library's lists changed while reading them"))?;
    if let Some(song) = &request.song {
        setlist
            .songs
            .retain(|s| s.slug.eq_ignore_ascii_case(song) || s.title.eq_ignore_ascii_case(song));
        if setlist.songs.is_empty() {
            eyre::bail!("no song {song:?} in {:?}", request.setlist);
        }
    }
    tracing::info!(
        open.org = %org,
        open.songs = setlist.songs.len(),
        "set: opening a library setlist"
    );
    crate::stream_set::open(
        crate::stream_set::Remote::Library {
            library,
            setlist,
            name: account.name(),
        },
        progress,
    )
}

/// Open what is at `path` — a song (`.RPP`, `.session`, a song's folder) or
/// a setlist (`.setlist`, a folder of songs) — as the set: `SESSION_OPEN`.
///
/// # Errors
///
/// Another open is under way.
pub fn open_path(path: PathBuf) -> Result<(), Busy> {
    let title = path.file_stem().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    open_with(
        Progress::Opening { title },
        Box::new(move |_| {
            let songs = songs_at(&path)
                .ok_or_else(|| eyre::eyre!("{} is not a song or a setlist", path.display()))?;
            Setlist::open(&songs)
        }),
    )
}

/// Join a set Task keeps, by its live link (`link`), as `name`: its songs
/// stream in and play here — `SESSION_JOIN`.
///
/// # Errors
///
/// Another open is under way.
pub fn open_live(link: String, name: String) -> Result<(), Busy> {
    open_with(
        Progress::Joining { retry: None },
        Box::new(move |progress| {
            crate::stream_set::open(crate::stream_set::Remote::Live { link, name }, progress)
        }),
    )
}

/// A new, untitled song as the set — `SESSION_NEW`.
///
/// # Errors
///
/// Another open is under way.
pub fn open_new() -> Result<(), Busy> {
    open_with(
        Progress::Opening {
            title: crate::project_new::UNTITLED.to_owned(),
        },
        Box::new(|_| crate::project_new::setlist(None)),
    )
}

/// The songs at `target`: itself for a song, the songs a setlist names, the
/// song in a song's folder; `None` when it is none of those.
#[must_use]
pub fn songs_at(target: &Path) -> Option<Vec<PathBuf>> {
    use crate::setlist::{read_setlist, song_in};
    if target.is_dir() && !crate::open::is_session(target) {
        if let Some(song) = song_in(target) {
            return Some(vec![song]);
        }
    } else if !target
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("setlist"))
    {
        return Some(vec![target.to_path_buf()]);
    }
    read_setlist(target)
        .inspect_err(|e| tracing::error!(error = %e, "the setlist could not be read"))
        .ok()
}
