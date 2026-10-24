//! Drum editing's judgement — which of a drum's hits a grid line claims,
//! and where the fills are — is the editing engine's (`editing::drums`),
//! re-exported here. The lanes an edit is made on are Session's own.

pub use editing::drums::*;

/// The lane a track's edits are made on: its `EDIT` lane, once the kit
/// has been given one (`DRUMS_EDIT_LANES`) — whatever lane is playing,
/// so the comp is never cut while it is being listened to.
#[must_use]
pub fn edit_lane(track: &daw_proto::Track) -> Option<u32> {
    track
        .lane_names
        .iter()
        .position(|n| n.trim().eq_ignore_ascii_case("EDIT"))
        .and_then(|i| u32::try_from(i).ok())
        .filter(|&i| i < track.lane_count)
}

/// Whether `item` is one of the takes an edit of `track` works on: on its
/// `EDIT` lane if it has one, else on a lane that plays.
#[must_use]
pub fn is_edited(track: &daw_proto::Track, item: &daw_proto::Item) -> bool {
    edit_lane(track).map_or_else(
        || daw_ui::studio::project::on_playing_lane(track, item),
        |lane| item.fixed_lane.unwrap_or(0) == lane,
    )
}
