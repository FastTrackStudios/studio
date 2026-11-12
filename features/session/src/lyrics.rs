//! A song's lyrics: the model is `lyrics-ui`'s (one source with the
//! lyrics panel and Studio's share-page island), re-exported here; what
//! needs Session's DAW, putting the lines on a project's Lyrics track
//! ([`stamp_lines`]), is Session's.

pub use lyrics_ui::lyrics::*;

/// Put `lyrics` on the Lyrics track — made in the Keyflow folder, before
/// its closing HITS, if the song has none — in place of whatever lines
/// were there. Returns how many lines were stamped.
///
/// # Errors
///
/// A line's item could not be created or labelled.
pub fn stamp_lines<D>(
    daw: &D,
    project: daw::service::ProjectContext,
    lyrics: &Lyrics,
) -> daw_proto::DawResult<usize>
where
    D: daw::service::Tracks + daw::service::Items,
{
    use daw::service::{Duration, ItemRef, PositionInSeconds, TrackRef};
    let tracks = daw.all(project.clone());
    let track = match tracks.iter().find(|t| is_lyrics_track(&t.name)) {
        Some(track) => TrackRef::Guid(track.guid.clone()),
        None => {
            let before_hits = tracks
                .iter()
                .find(|t| t.name.trim().eq_ignore_ascii_case("HITS"))
                .map(|t| t.index);
            TrackRef::Guid(daw.add(project.clone(), LYRICS_TRACK, before_hits)?)
        }
    };
    for item in daw.get_items(project.clone(), track.clone()) {
        daw.delete_item(project.clone(), ItemRef::Guid(item.guid))?;
    }
    for line in &lyrics.lines {
        let guid = daw
            .add_item(
                project.clone(),
                track.clone(),
                PositionInSeconds::from_seconds(line.start),
                Duration::from_seconds((line.end - line.start).max(0.05)),
            )
            .ok_or_else(|| {
                daw_proto::DawError::OperationFailed(format!(
                    "could not create a line at {:.2} s",
                    line.start
                ))
            })?;
        daw.set_label(project.clone(), ItemRef::Guid(guid), &line.text)?;
    }
    Ok(lyrics.lines.len())
}
