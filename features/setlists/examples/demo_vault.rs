//! A small vault to open in a build under test: a library of two songs,
//! each with the fixture chart (`tests/fixtures/joy.kf`) and a recording
//! you give it, and a setlist of them. What the iOS app is checked on
//! (docs/studio-ios.md): the vault's setlists, a song's chart and lyrics,
//! and an Ogg Vorbis recording played by the app's own decoder.
//!
//! ```sh
//! ffmpeg -f lavfi -i "sine=frequency=440:duration=45" -c:a libvorbis one.ogg
//! ffmpeg -f lavfi -i "sine=frequency=660:duration=45" -c:a libvorbis two.ogg
//! cargo run -p setlists --features engrave --example demo_vault -- <vault> one.ogg two.ogg
//! ```
//!
//! The songs are named for their recordings' file names.
#![allow(clippy::unwrap_used, clippy::print_stdout, clippy::expect_used)]

use std::path::PathBuf;

use setlists::chart::{SongFiles, import_song};
use setlists::layout::{Library, SongRef, setlist_note};
use setlists::setlist::Setlist;
use setlists::vault::Vault;

const CHART: &str = include_str!("../tests/fixtures/joy.kf");
const LIBRARY: &str = "Demo";
const SETLIST: &str = "Demo Set";

fn main() {
    let mut args = std::env::args().skip(1).map(PathBuf::from);
    let root = args.next().expect("usage: demo_vault <vault> <recording>…");
    let recordings: Vec<PathBuf> = args.collect();
    assert!(!recordings.is_empty(), "give at least one recording");
    std::fs::create_dir_all(&root).unwrap();
    let vault = Vault::at(&root);
    let lib = Library::named(LIBRARY);
    let scratch = root.join(".demo-vault");
    std::fs::create_dir_all(&scratch).unwrap();

    let mut set = Setlist::new(SETLIST, Vec::new());
    let mut known = Vec::new();
    for recording in &recordings {
        let title = recording
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        // The fixture's chart under this song's title: its first line is
        // `<title> - <artist>`.
        let (_, rest) = CHART.split_once('\n').unwrap();
        let kf = scratch.join(format!("{title}.kf"));
        std::fs::write(&kf, format!("{title} - Demo\n{rest}")).unwrap();
        let song = import_song(
            &vault,
            &lib,
            &SongFiles {
                chart: Some(&kf),
                media: &[recording.as_path()],
                ..SongFiles::default()
            },
            Some(&title),
        )
        .unwrap();
        let at = SongRef::new(LIBRARY, &title, None);
        set.add(at.clone(), None, None).unwrap();
        known.push((at, song));
    }
    vault
        .write(&setlist_note(&set.title), set.note(&known).as_bytes())
        .unwrap();
    std::fs::remove_dir_all(&scratch).unwrap();
    println!("{}", root.display());
}
