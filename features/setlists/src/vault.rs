//! Bringing songs into a vault on this machine.
//!
//! Their files are copied into the song's folder and its note written, in
//! place on disk. The Architect app or node holding the folder picks a
//! change up from disk and syncs it as it does any note.
//!
//! This is the one thing done on disk, because a song's files (its
//! session, its media) come from outside the vault. Everything else
//! (setlists, sharing, browsing) the extension does through the app's
//! actions ([`crate::ext`]). Every write goes to a temporary file first
//! and is renamed over the old one, so a watcher never reads half a note.

use std::path::{Path, PathBuf};

use crate::layout::{LIBRARIES, Library};
use crate::song::{Song, SongPage};
use crate::{Error, Result, check_name};

/// The type a library's own note has.
pub const LIBRARY_KIND: &str = "song-library";

/// A vault: a folder of notes.
#[derive(Debug, Clone)]
pub struct Vault {
    root: PathBuf,
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.display().to_string(),
        source,
    }
}

impl Vault {
    #[must_use]
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The file at vault path `rel`.
    #[must_use]
    pub fn path(&self, rel: &str) -> PathBuf {
        rel.split('/')
            .filter(|p| !p.is_empty())
            .fold(self.root.clone(), |p, part| p.join(part))
    }

    #[must_use]
    pub fn exists(&self, rel: &str) -> bool {
        self.path(rel).exists()
    }

    /// The text at vault path `rel`.
    ///
    /// # Errors
    ///
    /// It could not be read.
    pub fn read(&self, rel: &str) -> Result<String> {
        let path = self.path(rel);
        std::fs::read_to_string(&path).map_err(io(&path))
    }

    /// Write `bytes` at vault path `rel`, its folders made as needed.
    /// Unchanged bytes are not written again, so the app sees no edit.
    ///
    /// # Errors
    ///
    /// It could not be written.
    pub fn write(&self, rel: &str, bytes: &[u8]) -> Result<()> {
        let path = self.path(rel);
        if std::fs::read(&path).is_ok_and(|old| old == bytes) {
            return Ok(());
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io(dir))?;
        }
        let tmp = path.with_extension("setlists-tmp");
        std::fs::write(&tmp, bytes).map_err(io(&tmp))?;
        std::fs::rename(&tmp, &path).map_err(io(&path))
    }

    /// Copy the file at `from` to vault path `rel`.
    ///
    /// # Errors
    ///
    /// It could not be read or written.
    pub fn copy_in(&self, from: &Path, rel: &str) -> Result<()> {
        let bytes = std::fs::read(from).map_err(io(from))?;
        self.write(rel, &bytes)
    }

    /// Remove the file at vault path `rel`, if it is there.
    ///
    /// # Errors
    ///
    /// It is there and could not be removed.
    pub fn remove(&self, rel: &str) -> Result<()> {
        let path = self.path(rel);
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(io(&path)(e)),
            _ => Ok(()),
        }
    }

    /// Make `lib`'s own note if it has none: its songs by category, as a
    /// base Architect draws (in the app and on a site alike).
    ///
    /// # Errors
    ///
    /// It could not be written.
    pub fn ensure_library(&self, lib: &Library) -> Result<()> {
        if self.exists(&lib.note()) {
            return Ok(());
        }
        let note = format!(
            "---\ntype: {LIBRARY_KIND}\ntitle: {name}\n---\n# {name}\n\nA song library: every song with everything of its own, by category.\n\n```base\nfilters:\n  and:\n    - type == \"song\"\n    - file.inFolder(\"{folder}\")\nviews:\n  - type: table\n    name: Songs\n    groupBy: categories\n    order:\n      - file.name\n      - key\n      - tempo\n      - meter\n      - artist\n```\n",
            name = lib.name,
            folder = lib.folder,
        );
        self.write(&lib.note(), note.as_bytes())
    }

    fn dirs_in(&self, rel: &str) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.path(rel))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .filter(|n| !n.starts_with('.'))
            .collect();
        names.sort();
        names
    }

    /// The libraries in the vault.
    #[must_use]
    pub fn libraries(&self) -> Vec<Library> {
        self.dirs_in(LIBRARIES)
            .iter()
            .map(|n| Library::named(n))
            .collect()
    }

    /// Every song of `lib`, by title.
    #[must_use]
    pub fn songs(&self, lib: &Library) -> Vec<Song> {
        self.dirs_in(&lib.folder)
            .iter()
            .filter_map(|title| self.song(lib, title).ok())
            .collect()
    }

    /// Song `title` of `lib`.
    ///
    /// # Errors
    ///
    /// [`Error::NoSong`] when it has no song note.
    pub fn song(&self, lib: &Library, title: &str) -> Result<Song> {
        let rel = lib.song_note(title);
        let text = self
            .read(&rel)
            .map_err(|_| Error::NoSong(title.to_owned()))?;
        Song::from_note(lib, &text).ok_or(Error::NotA(rel, "song"))
    }

    /// Write `song`'s note.
    ///
    /// # Errors
    ///
    /// Its title is not a usable name, or it could not be written.
    pub fn write_song(&self, lib: &Library, song: &Song, page: &SongPage) -> Result<()> {
        check_name(&song.title)?;
        self.write(&lib.song_note(&song.title), song.note(lib, page).as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn songs_are_kept_by_library() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::at(dir.path());
        let lib = Library::named("Christmas");
        vault.ensure_library(&lib).unwrap();
        for (title, cats) in [("Joy to the World", vec!["Hymns"]), ("Sleigh Ride", vec![])] {
            let song = Song {
                title: title.to_owned(),
                categories: cats.into_iter().map(str::to_owned).collect(),
                ..Song::default()
            };
            vault.write_song(&lib, &song, &SongPage::default()).unwrap();
        }
        assert_eq!(vault.libraries(), vec![lib.clone()]);
        let songs = vault.songs(&lib);
        assert_eq!(songs.len(), 2);
        assert_eq!(songs[0].categories, ["Hymns"]);
        assert!(
            vault
                .read(&lib.note())
                .unwrap()
                .contains("groupBy: categories")
        );
        assert!(matches!(vault.song(&lib, "Nope"), Err(Error::NoSong(_))));
    }
}
