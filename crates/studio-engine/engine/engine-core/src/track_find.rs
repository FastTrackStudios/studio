//! A track named the way a person or an agent names one: its GUID, its
//! path or its name. Pure, so a page resolves tracks the same way as the
//! desktop engine.

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
