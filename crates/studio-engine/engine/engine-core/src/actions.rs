//! What every domain's actions share: the open song as an action reads it
//! ([`song`]), a track named by GUID, path or name ([`resolve`]), an
//! action's metadata ([`meta`]) and the Action List ([`list`]).

use daw::service::{ProjectContext, Tracks};

/// An action's metadata, in its place on the list: the client's.
pub use engine_runtime::client::actions::meta;

/// The Action List, as the engine filled it. The engine (studio-engine)
/// fills it as it comes up (`studio_engine::install`); an action running
/// here runs on a list already filled.
#[must_use]
pub fn list() -> &'static architect::action::ActionList {
    engine_runtime::client::actions::list()
}

/// The engine and the open song, as a pair — for the call sites not yet
/// on [`crate::song::Song`], which new code takes instead.
///
/// # Errors
///
/// No song is open.
pub fn song() -> Result<(&'static daw::standalone::Standalone, ProjectContext), String> {
    let song = crate::song::Song::current()?;
    Ok((song.daw(), song.context()))
}

/// A track by its GUID, its path (`Kick/Sum/Trig`, any tail of it) or its
/// name — refused, naming the candidates, when more than one fits.
///
/// # Errors
///
/// No track fits, or more than one does.
pub fn resolve(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    what: &str,
) -> Result<String, String> {
    resolve_in(&Tracks::all(daw, project.clone()), what)
}

pub use crate::track_find::resolve_in;
