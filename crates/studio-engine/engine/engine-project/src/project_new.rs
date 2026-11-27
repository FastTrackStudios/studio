//! New Project: a song from nothing (`SESSION_NEW`).
//!
//! An untitled song in the window's own engine, with what every song
//! starts from already placed: COUNT-IN on bar 1, SONGSTART after the
//! two-bar count-in keyflow's insert gives one, SONGEND [`SONG_BARS`] bars
//! on, and the guide (click, count, cues) generated over them. The window
//! is asked into Organize mode, where the structure and the instruments
//! go in.
//!
//! The song has no file until it is first saved: `SESSION_SAVE` then needs
//! a `to`, and the stem of `to` becomes its name ([`named`]).
//!
//! `SESSION_NEW` (`crate::session_new`: a window's New Project, the CLI, a
//! model) makes it the engine's set, a set of one ([`setlist_titled`],
//! `crate::studio_engine::set::open_new`); every window shows it, given
//! what was asked of it.

use daw::service::transport::service::Transport as _;
use daw::service::{ProjectContext, TempoMap as _};
use fts_session::section_kinds::{KeyflowAction, MarkerKind};

/// What a new song is called until it is saved.
pub const UNTITLED: &str = "Untitled";

/// The count-in's bars before SONGSTART — keyflow's, as its COUNT-IN
/// insert moves the cursor on by.
const COUNT_IN_BARS: i32 = 2;

/// The bars from SONGSTART to SONGEND in a new song: something for the
/// click to run over. A section inserted past them moves SONGEND out
/// (`crate::organize::insert`).
const SONG_BARS: i32 = 8;

/// Make a new, untitled song in the window's engine and make it current.
///
/// Its markers placed, its guide generated, Organize mode asked for.
/// `audio` as [`crate::open::open_untitled`]. Returns its project GUID.
///
/// # Errors
///
/// The window has no engine of its own, or the song's start could not be
/// placed.
pub fn create(audio: Option<bool>) -> Result<String, String> {
    let opened = crate::open::open_untitled(UNTITLED, audio).map_err(|e| e.to_string())?;
    // Placing the start moves the transport, which spawns on the engine's
    // runtime: run inside it, whichever thread the action came from.
    let runtime = crate::open::engine_runtime().map_err(|e| e.to_string())?;
    let _inside = runtime.enter();
    let project = ProjectContext::Project(opened.project_guid.clone());
    stamp(&opened.daw, &project)?;
    engine_domain::guide::Guide::new(opened.daw.clone())
        .with_instrument(crate::prepare::GUIDE_INSTRUMENT)
        .generate(engine_domain::guide::GuideScope::All)
        .map_err(|e| format!("the guide: {e}"))?;
    engine_runtime::client::actions::request_mode(engine_domain::modes::Mode::Organize);
    crate::song_model::request_resync();
    crate::actions::list().changed("project", UNTITLED);
    tracing::info!(
        song.project = %opened.project_guid,
        song.new = true,
        "a new song was made"
    );
    Ok(opened.project_guid)
}

/// COUNT-IN, SONGSTART and SONGEND, each placed by keyflow's own insert
/// (the toolbar's), the edit cursor left on SONGSTART: where the first
/// section goes.
fn stamp(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Result<(), String> {
    let bar = |measure: i32| daw.musical_to_time(project.clone(), measure, 1, 0.0);
    let start = bar(COUNT_IN_BARS.saturating_add(1));
    let end = bar(COUNT_IN_BARS.saturating_add(1).saturating_add(SONG_BARS));
    for (kind, at) in [
        (MarkerKind::CountIn, 0.0),
        (MarkerKind::SongStart, start),
        (MarkerKind::SongEnd, end),
    ] {
        daw.set_position(project.clone(), at)
            .map_err(|e| format!("the cursor would not move to {at:.3} s: {e}"))?;
        // The insert acts on the current song, which this one now is.
        engine_domain::keyflow::actions::dispatch(daw, KeyflowAction::InsertMarker(kind));
    }
    daw.set_position(project.clone(), start)
        .map_err(|e| format!("the cursor would not move to SONGSTART: {e}"))?;
    crate::edit_state::ask(crate::edit_state::Edit {
        at: start,
        selection: None,
    });
    Ok(())
}

/// Whether song `guid` has no file yet: the core's (`engine_core::open`).
pub use engine_core::open::untitled;

/// The song `guid`, saved for the first time as `to` (into the file
/// `at`): named for `to`, and `at` its own file from now on.
pub fn named(
    daw: &daw::standalone::Standalone,
    guid: &str,
    to: &std::path::Path,
    at: &std::path::Path,
) {
    let name = to
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| UNTITLED.to_owned());
    let path = at.to_string_lossy().into_owned();
    daw.write_project(guid, |p| {
        p.info.name = name;
        p.info.path = path;
    });
}

/// The song `guid` — current in the engine, as [`create`] leaves it — as
/// a set of one, the song called `title`.
///
/// # Errors
///
/// It could not be read back.
pub fn setlist_titled(title: String, guid: String) -> eyre::Result<crate::setlist::Setlist> {
    let session = crate::song_model::StudioSession::read_current(None, None)?;
    Ok(crate::setlist::Setlist::of(vec![crate::setlist::Song::of(
        title, guid, session,
    )]))
}
