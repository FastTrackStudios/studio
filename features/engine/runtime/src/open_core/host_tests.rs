//! The seam: what opening a song asks of its host ([`OpenHost`]), with
//! fakes in place of Session's `Prepare` and project files.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use daw::standalone::Standalone;

use super::{AsItIs, Media, OpenHost, Opened, open_song_in};
use crate::folder::{Folder, Memory};

const RPP: &str = "<REAPER_PROJECT 0.1 \"7.0/test\" 1700000000\n  SAMPLERATE 48000 0 0\n  TEMPO 120 4 4 0\n  <TRACK {0B5E0000-0000-0000-0000-000000000001}\n    NAME Guitar\n  >\n  <TRACK {0B5E0000-0000-0000-0000-000000000002}\n    NAME Bass\n  >\n>\n";

/// A song's folder that can be written back to (as a disk can), held in
/// memory.
struct Writable(Memory);

impl Folder for Writable {
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        self.0.read(path)
    }
    fn list(&self, dir: &Path) -> std::io::Result<Vec<String>> {
        self.0.list(dir)
    }
    fn is_dir(&self, path: &Path) -> bool {
        self.0.is_dir(path)
    }
    fn writable(&self) -> bool {
        true
    }
}

/// A host that prepares (or fails to) and saves, and says what it was asked.
#[derive(Default)]
struct Fake {
    fail_prepare: bool,
    prepared: Mutex<Vec<String>>,
    saved: Mutex<Vec<PathBuf>>,
}

impl OpenHost for Fake {
    fn nothing_to_prepare(&self) -> bool {
        false
    }
    fn prepare(&self, _folder: &dyn Folder, opened: &Opened) -> eyre::Result<()> {
        self.prepared
            .lock()
            .expect("the record")
            .push(opened.name.clone());
        if self.fail_prepare {
            return Err(eyre::eyre!("the chart would not build"));
        }
        Ok(())
    }
    fn save_session(&self, _opened: &Opened, to: &Path) -> Option<eyre::Result<PathBuf>> {
        self.saved.lock().ok()?.push(to.to_path_buf());
        Some(Ok(to.to_path_buf()))
    }
}

fn song_folder() -> Memory {
    let folder = Memory::new();
    folder.insert("/songs/Washed/Washed.RPP", RPP.as_bytes().to_vec());
    folder
}

#[test]
fn a_song_opens_from_memory_with_no_host_at_all() {
    let daw = Standalone::new();
    let (opened, plan) = open_song_in(
        &song_folder(),
        &daw,
        Path::new("/songs/Washed/Washed.RPP"),
        &AsItIs,
        Media::Deferred,
    )
    .expect("the song opens");
    assert_eq!(opened.name, "Washed");
    assert_eq!(opened.track_count, 2);
    assert!(!plan.prepare, "nothing to prepare: it opens as it is");
    assert_eq!(plan.save_to, None, "a browser's copy saves nothing");
}

#[test]
fn the_host_prepares_the_song_once_and_saves_it_as_its_session() {
    let host = Fake::default();
    let daw = Standalone::new();
    let (_, plan) = open_song_in(
        &Writable(song_folder()),
        &daw,
        Path::new("/songs/Washed/Washed.RPP"),
        &host,
        Media::Deferred,
    )
    .expect("the song opens");
    assert!(plan.prepare);
    assert_eq!(*host.prepared.lock().expect("the record"), ["Washed"]);
    assert_eq!(
        *host.saved.lock().expect("the record"),
        [PathBuf::from("/songs/Washed/Washed.session")],
        "saved beside the .RPP"
    );
}

#[test]
fn a_preparation_that_fails_saves_nothing() {
    let host = Fake {
        fail_prepare: true,
        ..Fake::default()
    };
    let daw = Standalone::new();
    let (opened, _) = open_song_in(
        &Writable(song_folder()),
        &daw,
        Path::new("/songs/Washed/Washed.RPP"),
        &host,
        Media::Deferred,
    )
    .expect("the song opens");
    assert_eq!(opened.track_count, 2, "it opens as it was");
    assert!(host.saved.lock().expect("the record").is_empty());
}

/// A host whose project files open through it, never as `.RPP` text.
struct ProjectFiles;

impl OpenHost for ProjectFiles {
    fn nothing_to_prepare(&self) -> bool {
        true
    }
    fn prepare(&self, _folder: &dyn Folder, _opened: &Opened) -> eyre::Result<()> {
        Ok(())
    }
    fn is_project_file(&self, path: &Path) -> bool {
        path.extension().is_some_and(|e| e == "song")
    }
    fn open_project_file(
        &self,
        daw: &Standalone,
        path: &Path,
        media: Media,
    ) -> Option<eyre::Result<Opened>> {
        if !self.is_project_file(path) {
            return None;
        }
        // The project file is the same song, kept elsewhere.
        Some(super::load_in(
            &song_folder(),
            daw,
            Path::new("/songs/Washed/Washed.RPP"),
            media,
            &AsItIs,
        ))
    }
}

#[test]
fn a_project_file_opens_through_its_host_as_it_is() {
    let daw = Standalone::new();
    let (opened, plan) = open_song_in(
        &Writable(Memory::new()),
        &daw,
        Path::new("/songs/Washed.song"),
        &ProjectFiles,
        Media::Deferred,
    )
    .expect("the song opens");
    assert_eq!(opened.track_count, 2);
    assert_eq!(plan.open, PathBuf::from("/songs/Washed.song"));
    assert!(
        !plan.prepare && plan.save_to.is_none(),
        "never prepared again"
    );
}
