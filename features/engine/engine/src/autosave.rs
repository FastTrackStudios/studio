//! Nothing in a song stays unsaved: every open song is written to its
//! project file shortly after it changes, and what happened to it is
//! logged beside the file.
//!
//! A thread looks at every song the engine has open (the set's, else the
//! one song) every few seconds. A song whose fingerprint
//! (`daw::standalone::digest::fingerprint`) differs from the one it was
//! last saved with, and has held still for one look (so a drag or a run
//! of edits is one save, not twenty), is saved into its `Song.session` as
//! one step of its history, named `Autosave`.
//!
//! Beside each file, `Song.session.log` gets a line for every time the
//! song is seen open for the first time in this engine (`opened`), every
//! autosave (`saved`), and the engine's end when it ends in order
//! (`closed`): the moment and the song's fingerprint then, so the state a
//! song was opened and left in can always be found in its history.

use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use daw::standalone::Standalone;

/// How often the open songs are looked at.
const EVERY: Duration = Duration::from_secs(2);

/// What is known of one open song.
struct Seen {
    /// Its project file.
    file: PathBuf,
    /// Its fingerprint as last saved (or as first seen).
    saved: u64,
    /// Its fingerprint at the last look.
    last: u64,
}

/// Start saving behind every open song, for as long as the process runs.
/// Once: a second call does nothing.
pub fn start() {
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if STARTED.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("autosave".to_owned())
        .spawn(|| {
            let mut seen: HashMap<String, Seen> = HashMap::new();
            loop {
                std::thread::sleep(EVERY);
                let Some(daw) = crate::open::local_engine() else {
                    continue;
                };
                for guid in open_songs() {
                    look(daw, &guid, &mut seen);
                }
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "autosave: no thread to save on");
    }
}

/// Every song the engine has open: the set's, else the current one.
fn open_songs() -> Vec<String> {
    crate::studio_engine::set::lineup().map_or_else(
        || crate::open::current_song().into_iter().collect(),
        |lineup| lineup.songs.into_iter().map(|(_, guid)| guid).collect(),
    )
}

/// One look at one song: logged when first seen, saved when it changed
/// and has held still since the last look.
fn look(daw: &'static Standalone, guid: &str, seen: &mut HashMap<String, Seen>) {
    let Some(now) = daw::standalone::digest::fingerprint(daw, guid) else {
        return;
    };
    let Some(known) = seen.get_mut(guid) else {
        let Some(file) = file_of(daw, guid) else {
            return;
        };
        log(&file, "opened", now);
        seen.insert(
            guid.to_owned(),
            Seen {
                file,
                saved: now,
                last: now,
            },
        );
        return;
    };
    let still = known.last == now;
    known.last = now;
    if now == known.saved || !still {
        return;
    }
    let (file, branch) = crate::project_file::save_target(&known.file);
    match crate::project_file::save(daw, guid, &file, &branch, "Autosave") {
        Ok(_) => {
            crate::projects::mark_saved(daw, guid);
            known.saved = now;
            log(&known.file, "saved", now);
        }
        Err(error) => {
            tracing::warn!(song.file = %known.file.display(), %error, "autosave: the song was not saved");
        }
    }
}

/// The song's project file: the `.session` it was opened from, or the one
/// beside the file it was (`Song.RPP` to `Song.session`). None for a song
/// with no file yet.
fn file_of(daw: &Standalone, guid: &str) -> Option<PathBuf> {
    if crate::project_new::untitled(daw, guid) {
        return None;
    }
    let from = PathBuf::from(daw.read_project(guid, |p| p.info.path.clone())?);
    if from.as_os_str().is_empty() {
        return None;
    }
    Some(if from.extension().is_some_and(|e| e == "session") {
        from
    } else {
        from.with_extension("session")
    })
}

/// A line in the song's log: when, what, and its fingerprint then.
fn log(file: &Path, what: &str, fingerprint: u64) {
    let path = PathBuf::from(format!("{}.log", file.display()));
    let at = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let line = format!(
        "{{\"at\":{at},\"event\":\"{what}\",\"fingerprint\":\"{fingerprint:016x}\",\"engine\":{}}}\n",
        std::process::id()
    );
    let written = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(line.as_bytes()));
    if let Err(error) = written {
        tracing::debug!(song.log = %path.display(), %error, "autosave: the log line was not written");
    }
}

/// Every open song's log gets `closed`: called as the engine ends in
/// order.
pub fn closing() {
    let Some(daw) = crate::open::local_engine() else {
        return;
    };
    for guid in open_songs() {
        if let (Some(file), Some(now)) = (
            file_of(daw, &guid),
            daw::standalone::digest::fingerprint(daw, &guid),
        ) {
            log(&file, "closed", now);
        }
    }
}
