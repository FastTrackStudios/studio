//! A new song from nothing: `<folder>/<Title>.session` with a minted id,
//! an empty engine project (at most its tempo) and the session seeded from
//! that project's timing, the way [`crate::convert`] seeds a lone store.
//!
//! For a song whose material is not in the session's form yet (a REAPER
//! project, a chart, lyrics kept in the folder's `Inbox/`): the song gets
//! its id now, so a setlist can name it, and its material is converted
//! into it later.
//!
//! The file is built in a scratch folder, read back from its bytes alone
//! (the id, the session), written beside the target as
//! `<Title>.session.new`, compared, and only then renamed into place. A folder that already holds a song is refused; nothing is ever
//! overwritten.

use std::path::{Path, PathBuf};

use session_format::read::ProjectFile;
use session_format::store::{MAIN, SESSION, Store};

use crate::convert::{SessionSummary, session_from_project};
use crate::{EXTENSION, Error, Layout, Result, file_error};

/// The tempo a new song starts at: one point at the start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tempo {
    pub bpm: f64,
    pub beats_per_bar: u32,
    pub beat_unit: u32,
}

/// What [`new_song`] made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Made {
    pub file: PathBuf,
    pub id: String,
    pub session: SessionSummary,
}

/// Make `<folder>/<title>.session`: a new id, and `tempo` (else the
/// session's default) in the engine's project and the session.
///
/// # Errors
///
/// The title is not a file name, the folder already holds a song (or a
/// build stopped half way: its `.session.new` is there), or the new file
/// does not read back as written.
pub fn new_song(folder: &Path, title: &str, tempo: Option<Tempo>) -> Result<Made> {
    let _span = tracing::info_span!("song_file.new", song.id = tracing::field::Empty).entered();
    let title = title.trim();
    if title.is_empty() || title.contains(['/', '\\']) || title.starts_with('.') {
        return Err(Error::Refused(format!(
            "{title:?} is not a song's file name"
        )));
    }
    if let Some(t) = tempo
        && !(t.bpm.is_finite() && t.bpm > 0.0 && t.beats_per_bar > 0 && t.beat_unit > 0)
    {
        return Err(Error::Refused(format!("{t:?} is not a tempo")));
    }
    std::fs::create_dir_all(folder)?;
    if Layout::of(folder)? != Layout::Empty {
        return Err(file_error(folder, "holds a song already: nothing made"));
    }
    let file = folder.join(format!("{title}.{EXTENSION}"));
    let new = folder.join(format!("{title}.{EXTENSION}.new"));
    if file.exists() || new.exists() {
        return Err(file_error(
            &new,
            "is there already (a build stopped half way?): not overwritten",
        ));
    }

    // The engine's project: its tempo only.
    let engine = session::sync::SessionDoc::new();
    let mut model = session::sync::model::SessionModel::default();
    if let Some(t) = tempo {
        model.tempo = vec![session::sync::model::TempoPoint {
            at: 0.0,
            bpm: t.bpm,
            beats_per_bar: t.beats_per_bar,
            beat_unit: t.beat_unit,
        }];
    }
    let err = |e: &dyn std::fmt::Display| Error::Session(e.to_string());
    engine.write(&model, "song-file").map_err(|e| err(&e))?;
    let project = loro::LoroDoc::new();
    project
        .import(&engine.snapshot().map_err(|e| err(&e))?)
        .map_err(|e| err(&e))?;
    let seeded = session_from_project(&project)?;
    // Built in a scratch folder (closing a store leaves its log and shared
    // memory beside it, empty), so only the one file comes into the song's.
    let scratch = tempfile::tempdir()?;
    let built = scratch.path().join(format!("{title}.{EXTENSION}"));
    {
        let store = Store::open(&built).map_err(|e| file_error(&built, e))?;
        let updates = engine
            .updates_since(&Default::default())
            .map_err(|e| err(&e))?;
        store.append_update(MAIN, "New song", &updates)?;
        store.ensure_song_id()?;
        store.compact_part(SESSION, MAIN, &seeded)?;
        store.close()?;
    }

    // Read back from the bytes alone: the file is the whole song.
    let bytes = std::fs::read(&built)?;
    let read = ProjectFile::read(&bytes).map_err(|e| Error::Mismatch(e.to_string()))?;
    let id = read
        .song_id()
        .ok_or_else(|| Error::Mismatch("the new file has no id".into()))?
        .to_owned();
    let part = read
        .part(SESSION, MAIN)
        .map_err(|e| Error::Mismatch(e.to_string()))?;
    if part.get_deep_value() != seeded.get_deep_value() {
        return Err(Error::Mismatch("the seeded session differs".into()));
    }
    tracing::Span::current().record("song.id", id.as_str());
    std::fs::write(&new, &bytes)?;
    if std::fs::read(&new)? != bytes {
        return Err(Error::Mismatch(format!(
            "{} does not read back as written",
            new.display()
        )));
    }
    std::fs::rename(&new, &file)?;
    Ok(Made {
        file,
        id,
        session: SessionSummary::of(&part),
    })
}
