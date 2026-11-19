//! Print a project file's tempo map, one point a line:
//! `<file>\t<seconds>\t<bpm>\t<beats>/<unit>` — what a song's bars are
//! counted by, read straight from the `.session` (no engine).
//!
//! ```sh
//! session-tempo "Song/Song.session" …
//! ```

use std::path::PathBuf;

use session::sync::SessionDoc;
use session_format::store::{MAIN, Store};

fn main() -> eyre::Result<()> {
    let files: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if files.is_empty() {
        eyre::bail!("usage: session-tempo <file.session> …");
    }
    for file in files {
        let store = Store::open(&file).map_err(|e| eyre::eyre!("{}: {e}", file.display()))?;
        let doc = store
            .load(MAIN)
            .map_err(|e| eyre::eyre!("{}: {e}", file.display()))?;
        for point in SessionDoc::from_loro(doc).read().tempo {
            println!(
                "{}\t{:.6}\t{}\t{}/{}",
                file.display(),
                point.at,
                point.bpm,
                point.beats_per_bar,
                point.beat_unit
            );
        }
    }
    Ok(())
}
