//! What was handed to `studio ingest`, and the songs it holds.
//!
//! An input is a Live set to split (`.als`), one song (its folder, its
//! `.session` or its `.RPP`), or a folder of song folders (an album, a
//! setlist's folder). [`classify`] says which; [`songs_of`] lists the songs
//! in each, in order. What a song is called and its library id come from
//! its project file's name ([`identity`]).

use std::path::{Path, PathBuf};

/// One input, as [`classify`] read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// A show recorded as one Live set: split into songs first.
    Show(PathBuf),
    /// One song: its project file (`.session`, else `.RPP`).
    Song(PathBuf),
    /// A folder of song folders, each one song, in name order.
    Folder(PathBuf),
}

impl Input {
    /// The name the input goes by: the show's file, the folder's or the
    /// song's — what a setlist is called when nothing else names it.
    #[must_use]
    pub fn name(&self) -> String {
        let stem = |p: &Path| {
            p.file_stem()
                .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
        };
        match self {
            Self::Show(path) | Self::Song(path) => stem(path),
            Self::Folder(dir) => dir
                .file_name()
                .map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
        }
    }
}

/// A song to ingest: what it is called, its library id, its project file,
/// and the folder that is uploaded as its session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SongSource {
    pub title: String,
    pub slug: String,
    pub file: PathBuf,
    pub folder: PathBuf,
}

/// What `path` is, read from the disk.
///
/// # Errors
///
/// It is not there, or it is nothing ingest knows: not a Live set, a song
/// or a folder holding songs.
pub fn classify(path: &Path) -> eyre::Result<Input> {
    let meta = std::fs::metadata(path).map_err(|e| eyre::eyre!("{}: {e}", path.display()))?;
    if meta.is_file() {
        return match extension(path).as_str() {
            "als" => Ok(Input::Show(path.to_path_buf())),
            "session" | "rpp" => Ok(Input::Song(path.to_path_buf())),
            _ => Err(eyre::eyre!(
                "{}: not a Live set (.als), a .session or an .RPP",
                path.display()
            )),
        };
    }
    if let Some(file) = crate::setlist::song_in(path) {
        return Ok(Input::Song(file));
    }
    let sets = files_with(path, "als")?;
    if let [set] = sets.as_slice() {
        return Ok(Input::Show(set.clone()));
    }
    if song_folders(path)?.is_empty() {
        eyre::bail!(
            "{}: no song in it (a .session or .RPP, a Live set, or folders holding them)",
            path.display()
        );
    }
    Ok(Input::Folder(path.to_path_buf()))
}

/// The songs of a song or a folder input, in order. A show's come from
/// splitting it (`super::prepare::split`).
///
/// # Errors
///
/// The folder could not be read.
pub fn songs_of(input: &Input) -> eyre::Result<Vec<SongSource>> {
    match input {
        Input::Show(_) => Ok(Vec::new()),
        Input::Song(file) => Ok(vec![song_at(file)]),
        Input::Folder(dir) => Ok(song_folders(dir)?
            .into_iter()
            .map(|(_, file)| song_at(&file))
            .collect()),
    }
}

/// The song whose project file is `file`.
#[must_use]
pub fn song_at(file: &Path) -> SongSource {
    let (title, slug) = identity(
        &file
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
    );
    SongSource {
        title,
        slug,
        file: file.to_path_buf(),
        folder: file
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf),
    }
}

/// A song's title and library id from its project file's name: the
/// name, a leading track number dropped (`05 Empty Focus` → `Empty
/// Focus`, `empty-focus`).
#[must_use]
pub fn identity(stem: &str) -> (String, String) {
    let title = session::library::without_track_number(stem).to_owned();
    let slug = session::library::slugify(&title);
    (title, slug)
}

/// Two songs of one ingest with the same library id, if any: the first
/// pair, by title.
#[must_use]
pub fn clash(songs: &[SongSource]) -> Option<(&str, &str)> {
    songs.iter().enumerate().find_map(|(i, a)| {
        songs[i + 1..]
            .iter()
            .find(|b| b.slug == a.slug)
            .map(|b| (a.title.as_str(), b.title.as_str()))
    })
}

/// The folders directly in `dir` that are songs, with each one's project
/// file, in name order.
fn song_folders(dir: &Path) -> eyre::Result<Vec<(PathBuf, PathBuf)>> {
    let mut folders: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| eyre::eyre!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !is_hidden(p))
        .collect();
    folders.sort();
    Ok(folders
        .into_iter()
        .filter_map(|folder| crate::setlist::song_in(&folder).map(|file| (folder, file)))
        .collect())
}

fn files_with(dir: &Path, ext: &str) -> eyre::Result<Vec<PathBuf>> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| eyre::eyre!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && extension(p) == ext)
        .collect();
    found.sort();
    Ok(found)
}

fn extension(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with('.'))
}
