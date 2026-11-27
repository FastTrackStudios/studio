//! The set's openers: a path, a new song, and a song's colour by hand.
//! They need what the core does not own (a new project, the window's
//! opening of a song), so they stay here, beside the core's set
//! (`engine_core`'s `studio_engine::set`), under the same path
//! (`crate::studio_engine::set`). They go with engine-project
//! (`project_new`).

use std::path::{Path, PathBuf};

use engine_core::studio_engine::set::{Busy, open_with, position_named, update, view};

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

/// A new song as the set, made as `start` says — `SESSION_NEW`.
///
/// # Errors
///
/// Another open is under way.
pub fn open_new(start: crate::session_new::Start) -> Result<(), Busy> {
    open_with(
        Progress::Opening {
            title: start.title(),
        },
        Box::new(move |_| crate::session_new::make(start)),
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
