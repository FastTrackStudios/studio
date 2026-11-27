//! A chart written in Keyflow's language (a `.kf` file) brought into a
//! song's session, once.
//!
//! A song's chart lives in its session, the text entry [`crate::CHART`] ([`crate::written_chart`]).
//! Songs charted before that have a `.kf` file instead: beside the song
//! (`<Song>/<Title>.kf`), or an arrangement's in `Arrangements/<name>/`.
//! Whoever opens a song's session (the engine, on every platform that
//! opens a song from a file) imports that file into it with
//! [`import_chart`], and the session holds the chart from then on:
//!
//! - **Only into a session with no chart.** A session's own chart is never
//!   replaced. One that differs from the file is kept, and said
//!   ([`Outcome::Kept`]) so the host can log it.
//! - **One undo step**, named `Import chart from <file>` ([`step_name`]).
//! - **Idempotent.** Once imported, the session has a chart: a second open
//!   does nothing ([`Outcome::Same`]).
//! - **Only a chart that reads.** A file Keyflow cannot parse is refused
//!   ([`crate::Error::Chart`]), and the session is left as it was.
//!
//! Finding the file ([`kf_for`]) is native only: a browser has no folder
//! beside the song to look in.

#[cfg(not(target_family = "wasm"))]
use std::path::{Path, PathBuf};

use session::doc::SessionDoc;

use crate::{Error, Result, write_chart, written_chart};

/// The folder an arrangement's chart is kept in, under the song's folder:
/// `Arrangements/<name>/`.
pub const ARRANGEMENTS: &str = "Arrangements";

/// What [`import_chart`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The session had no chart. It holds the file's now, one undo step.
    Imported,
    /// The session's chart is the file's already. Nothing was done.
    Same,
    /// The session has a chart of its own, unlike the file's. It was kept.
    Kept,
}

impl Outcome {
    /// The outcome's name, for a log field or a report.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Imported => "imported",
            Self::Same => "same",
            Self::Kept => "kept",
        }
    }
}

/// The name of the undo step an import is: `Import chart from <file>`.
#[must_use]
pub fn step_name(file: &str) -> String {
    format!("Import chart from {file}")
}

/// Imports `source` (Keyflow's language, read from the file named `file`)
/// into `session` as its chart, when the session has none.
///
/// The write is one commit, the undo step [`step_name`]`(file)`. A session
/// with a chart is never written ([`Outcome::Same`], [`Outcome::Kept`]).
///
/// # Errors
///
/// [`Error::Chart`] when `source` is empty or does not parse as Keyflow
/// (the session is not touched); [`Error::Doc`] when the document refused
/// the write.
pub fn import_chart(session: &SessionDoc, file: &str, source: &str) -> Result<Outcome> {
    let held = written_chart(session);
    if !held.trim().is_empty() {
        return Ok(if held.trim_end() == source.trim_end() {
            Outcome::Same
        } else {
            Outcome::Kept
        });
    }
    if source.trim().is_empty() {
        return Err(Error::Chart(format!("{file} is empty")));
    }
    keyflow_lang::text::chart::parse_document(source)
        .map_err(|why| Error::Chart(format!("{file} does not read as Keyflow: {why}")))?;
    // The message names the commit `write_chart` makes: the one step.
    session.doc().set_next_commit_message(&step_name(file));
    write_chart(session, source)?;
    Ok(Outcome::Imported)
}

/// The `.kf` files that could be a song's chart.
#[cfg(not(target_family = "wasm"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// No `.kf` where the song's chart would be.
    Nothing,
    /// The song's chart.
    One(PathBuf),
    /// Several `.kf` files and none named for the song: which is its chart
    /// is not this code's guess to make.
    Several(Vec<PathBuf>),
}

/// The `.kf` that is the chart of the song whose file is `song` (its
/// `.session`), or of its arrangement `arrangement`.
///
/// The song's is in the song's folder, an arrangement's in
/// `Arrangements/<arrangement>/` under it. One `.kf` there is the chart.
/// Of several, the one named for the song (`<Title>.kf`, an arrangement's
/// `<Title> (<arrangement>).kf`) is.
#[cfg(not(target_family = "wasm"))]
#[must_use]
pub fn kf_for(song: &Path, arrangement: Option<&str>) -> Found {
    let Some(folder) = song.parent() else {
        return Found::Nothing;
    };
    let title = song
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (dir, named) = match arrangement {
        Some(name) => (
            folder.join(ARRANGEMENTS).join(name),
            format!("{title} ({name}).kf"),
        ),
        None => (folder.to_path_buf(), format!("{title}.kf")),
    };
    let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension().is_some_and(|x| x == "kf")
                && !p
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .collect();
    found.sort();
    match found.len() {
        0 => Found::Nothing,
        1 => Found::One(found.remove(0)),
        _ => match found
            .iter()
            .position(|p| p.file_name().is_some_and(|n| n.to_string_lossy() == named))
        {
            Some(i) => Found::One(found.remove(i)),
            None => Found::Several(found),
        },
    }
}

/// What [`import_beside`] found and did.
#[cfg(not(target_family = "wasm"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Beside {
    /// The `.kf` files looked at.
    pub found: Found,
    /// What the import did; `None` when there was no one file to import.
    pub outcome: Option<Outcome>,
}

/// Imports the `.kf` beside song `song` (or its arrangement's,
/// [`kf_for`]) into `session` ([`import_chart`]). A session that has a
/// chart is not read against a file at all when none is there.
///
/// # Errors
///
/// The file would not read, or [`import_chart`]'s.
#[cfg(not(target_family = "wasm"))]
pub fn import_beside(
    session: &SessionDoc,
    song: &Path,
    arrangement: Option<&str>,
) -> Result<Beside> {
    let found = kf_for(song, arrangement);
    let outcome = match &found {
        Found::One(path) => Some(import_file(session, path)?),
        Found::Nothing | Found::Several(_) => None,
    };
    Ok(Beside { found, outcome })
}

/// Imports the `.kf` at `path` into `session` ([`import_chart`]), the step
/// named by its file name.
///
/// # Errors
///
/// The file would not read, or [`import_chart`]'s.
#[cfg(not(target_family = "wasm"))]
pub fn import_file(session: &SessionDoc, path: &Path) -> Result<Outcome> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| Error::File(format!("{}: {e}", path.display())))?;
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    import_chart(session, &name, &source)
}
