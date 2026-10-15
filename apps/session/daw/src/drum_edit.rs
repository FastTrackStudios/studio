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

#[cfg(test)]
mod tests {
    use super::*;

    /// Once a track has an EDIT lane its edits are made there, whichever
    /// lane plays; before, on what plays.
    #[test]
    fn edits_are_made_on_the_edit_lane() {
        let item = |lane| daw_proto::Item {
            fixed_lane: Some(lane),
            ..daw_proto::Item::default()
        };
        let mut track = daw_proto::Track {
            lane_count: 3,
            lane_names: vec!["COMP".into(), "1".into(), "EDIT".into()],
            lane_play_mask: 1,
            ..daw_proto::Track::default()
        };
        assert!(is_edited(&track, &item(2)), "EDIT, with COMP playing");
        assert!(!is_edited(&track, &item(0)), "never the comp");
        track.lane_names = vec!["C1".into(), "1".into(), "2".into()];
        assert!(is_edited(&track, &item(0)), "no EDIT: what plays");
        assert!(!is_edited(&track, &item(2)));
    }
}
