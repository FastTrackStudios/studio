//! `SESSION_BACKUP`: a consistent copy of the open song's file, checked.
//!
//! The song's file is one SQLite database (`session_format::store`, on
//! turso) with a write-ahead log beside it. Turso has no online backup
//! API, so the copy is made the way that API would: holding the song (no
//! step is appended meanwhile, `project_file::with_open_store`), the log
//! folded into the file (`PRAGMA wal_checkpoint(TRUNCATE)`), the file
//! copied. The copy is then opened as a song file of its own and read —
//! its schema, its branches, its steps, the song's doc loaded whole: the
//! integrity check. By default it goes in `Backups/` beside the song,
//! named with the time.

use std::path::{Path, PathBuf};

use architect::action::{ActionList, ActionMeta, Effect};
use facet::Facet;
use session_format::store::{MAIN, Store};

static BACKUP: ActionMeta = crate::actions::meta(
    "SESSION_BACKUP",
    "session_backup",
    "Back up the song's file",
    "A consistent copy of the open song's file (`.session`, one SQLite database): made while no edit is written to it, its write-ahead log folded in first, then copied — to `to` (a file, or a folder to put it in), else a `Backups` folder beside the song, named `<song> <date time>.session`. The copy is then opened and read whole — its schema, branches, steps and the song itself — the integrity check. Says where it went, its size, and the check (`ok`, and why not). An existing file is never overwritten.",
    "File",
    "",
);

/// Where.
#[derive(Facet, Default)]
struct BackupArg {
    to: Option<String>,
}

/// What a backup came to.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct BackedUp {
    pub path: String,
    pub bytes: u64,
    pub check: Checked,
}

/// The copy's integrity check.
#[derive(Facet, Debug, Clone, PartialEq, Default)]
pub struct Checked {
    pub ok: bool,
    /// It opened as a song file of this build's schema.
    pub opened: bool,
    pub branches: u32,
    /// Steps on the song's own branch.
    pub steps: u32,
    /// The song's doc loaded whole.
    pub song_loaded: bool,
    pub why: Option<String>,
}

/// Where a backup of `song` goes: `to` as given (a folder: the default
/// name in it), else `Backups/` beside the song; named `<stem> <stamp>`.
#[must_use]
pub fn target(song: &Path, to: Option<&Path>, stamp: &str) -> PathBuf {
    let stem = song
        .file_stem()
        .map_or_else(|| "Song".to_owned(), |s| s.to_string_lossy().into_owned());
    let name = format!("{stem} {stamp}.session");
    match to {
        Some(to) if to.is_dir() || to.extension().is_none() => to.join(name),
        Some(to) => to.to_path_buf(),
        None => song
            .parent()
            .map_or_else(|| PathBuf::from("Backups"), |d| d.join("Backups"))
            .join(name),
    }
}

/// Copy the song file at `from` (its log folded in) to `to`, consistent:
/// `store` is the song's open store, held by the caller. Size of the copy.
///
/// # Errors
/// `to` exists, its folder cannot be made, the fold or the copy failed.
pub fn copy(store: &Store, from: &Path, to: &Path) -> Result<u64, String> {
    if to.exists() {
        return Err(format!(
            "{} is there already: not overwritten",
            to.display()
        ));
    }
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let step = tracing::info_span!("backup.fold").entered();
    store
        .checkpoint()
        .map_err(|e| format!("folding the log in: {e}"))?;
    drop(step);
    let step = tracing::info_span!("backup.copy").entered();
    let bytes = std::fs::copy(from, to).map_err(|e| format!("{}: {e}", to.display()))?;
    drop(step);
    Ok(bytes)
}

/// The copy at `path` opened and read whole.
#[must_use]
pub fn check(path: &Path) -> Checked {
    let _step = tracing::info_span!("backup.check").entered();
    let mut out = Checked::default();
    let store = match Store::open(path) {
        Ok(s) => s,
        Err(e) => {
            out.why = Some(format!("it does not open: {e}"));
            return out;
        }
    };
    out.opened = true;
    let read = (|| -> Result<(), String> {
        let branches = store.branches().map_err(|e| format!("its branches: {e}"))?;
        out.branches = u32::try_from(branches.len()).unwrap_or(u32::MAX);
        let steps = store.steps(MAIN).map_err(|e| format!("its steps: {e}"))?;
        out.steps = u32::try_from(steps.len()).unwrap_or(u32::MAX);
        store.load(MAIN).map_err(|e| format!("the song: {e}"))?;
        out.song_loaded = true;
        Ok(())
    })();
    if let Err(why) = read {
        out.why = Some(why);
    }
    if let Err(e) = store.close() {
        out.why.get_or_insert(format!("closing it: {e}"));
    }
    out.ok = out.opened && out.song_loaded && out.why.is_none();
    out
}

/// Back up the song open as `guid` to `to` (else beside it, `Backups/`).
///
/// # Errors
/// The song is not open from its file, or the copy failed.
pub fn backup(guid: &str, to: Option<&Path>) -> Result<BackedUp, String> {
    let stamp = chrono::Local::now().format("%Y-%m-%d %H%M%S").to_string();
    let to = to.map(Path::to_path_buf);
    let (path, bytes) = crate::project_file::with_open_store(guid, move |store, song| {
        let path = target(song, to.as_deref(), &stamp);
        let bytes = copy(store, song, &path)?;
        Ok((path, bytes))
    })?;
    let check = check(&path);
    if !check.ok {
        tracing::warn!(
            backup.why = check.why.as_deref().unwrap_or_default(),
            "a backup of the song failed its check"
        );
    }
    Ok(BackedUp {
        path: path.display().to_string(),
        bytes,
        check,
    })
}

/// `SESSION_BACKUP`.
///
/// # Errors
/// When it is already registered.
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&BACKUP, Effect::Mutating, |a: BackupArg| {
        let span = tracing::info_span!(
            "SESSION_BACKUP",
            backup.bytes = tracing::field::Empty,
            backup.ok = tracing::field::Empty,
        );
        let _in = span.enter();
        let (_, project) = crate::actions::song()?;
        let daw::service::ProjectContext::Project(guid) = project else {
            return Err("no song open".to_owned());
        };
        let done = backup(&guid, a.to.as_deref().map(Path::new))?;
        span.record("backup.bytes", done.bytes);
        span.record("backup.ok", done.check.ok);
        Ok(done)
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backup_goes_beside_the_song_by_default() {
        let at = target(
            Path::new("/songs/Unbreakable.session"),
            None,
            "2026-11-23 101500",
        );
        assert_eq!(
            at,
            PathBuf::from("/songs/Backups/Unbreakable 2026-11-23 101500.session")
        );
        let dir = tempfile::tempdir().expect("a dir");
        let into = target(Path::new("/songs/U.session"), Some(dir.path()), "x");
        assert_eq!(into, dir.path().join("U x.session"));
        let file = target(
            Path::new("/songs/U.session"),
            Some(Path::new("/b/copy.session")),
            "x",
        );
        assert_eq!(file, PathBuf::from("/b/copy.session"));
    }

    #[test]
    fn a_copy_of_a_live_store_opens_and_reads_whole() {
        let dir = tempfile::tempdir().expect("a dir");
        let song = dir.path().join("Song.session");
        let store = Store::open(&song).expect("a store");
        store.set_meta("probe", "1").expect("a write");
        let to = target(&song, None, "now");
        let bytes = copy(&store, &song, &to).expect("copied");
        assert!(bytes > 0);
        let checked = check(&to);
        assert!(checked.opened, "{checked:?}");
        assert!(checked.branches >= 1, "{checked:?}");
        // A second copy to the same place is refused, never overwritten.
        assert!(copy(&store, &song, &to).is_err());
        // Not a song file: the check says so.
        let junk = dir.path().join("junk.session");
        std::fs::write(&junk, b"not sqlite").expect("written");
        assert!(!check(&junk).ok);
    }
}
