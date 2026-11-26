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

/// The track `what` means among the song's `tracks`: its GUID, its path
/// (`Kick/Sum/Trig`, any tail of it) or its name — refused, naming the
/// candidates, when more than one fits.
///
/// # Errors
/// None fits, or more than one.
pub fn resolve_in(tracks: &[daw_proto::Track], what: &str) -> Result<String, String> {
    if let Some(t) = tracks.iter().find(|t| t.guid.eq_ignore_ascii_case(what)) {
        return Ok(t.guid.clone());
    }
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let want = what.trim().trim_matches('/').to_lowercase();
    let fits: Vec<(&daw_proto::Track, String)> = tracks
        .iter()
        .map(|t| {
            (
                t,
                engine_runtime::client::gain_stage::path_of(t, &by_guid).join("/"),
            )
        })
        .filter(|(_, path)| {
            let path = path.to_lowercase();
            path == want || path.ends_with(&format!("/{want}"))
        })
        .collect();
    // A whole path is that track, however many others end the same way:
    // `Drums` is the top-level folder, not `REFERENCES/…/Drums` too.
    let exact: Vec<&(&daw_proto::Track, String)> = fits
        .iter()
        .filter(|(_, p)| p.to_lowercase() == want)
        .collect();
    if let [(only, _)] = exact.as_slice() {
        return Ok(only.guid.clone());
    }
    match fits.as_slice() {
        [(only, _)] => Ok(only.guid.clone()),
        [] => Err(format!("no track {what} — list them with SESSION_TRACKS")),
        many => Err(format!(
            "{what} is more than one track: {:?} — give more of its path",
            many.iter().map(|(_, p)| p.as_str()).collect::<Vec<_>>()
        )),
    }
}
