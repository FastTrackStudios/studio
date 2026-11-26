//! The vocal leveler's lanes as a track keeps them, and the items it
//! levels: what the mix's automation and the listen domain read. Moved
//! down from engine-edit's `vocal_level`, which re-exports it.

use daw::service::{Items as _, ProjectContext, TrackRef, Tracks};

/// What ends a track's ext key holding one item's lanes.
pub const LANES_KEY: &str = "FTS:level:";

/// The items of `track` that play: not muted, on a lane that plays.
pub fn playing(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &daw_proto::Track,
) -> Vec<daw_proto::Item> {
    daw.get_items(project.clone(), TrackRef::Guid(track.guid.clone()))
        .into_iter()
        .filter(|i| !i.muted)
        .filter(|i| {
            i.fixed_lane
                .is_none_or(|lane| lane < 64 && track.lane_play_mask & (1_u64 << lane) != 0)
                || track.lane_play_mask == 0
        })
        .collect()
}

/// The track ext key that shows the leveler's lanes under a track.
pub const LANE_SHOWN: &str = "FTS:level:lane";

/// Whether `track` shows the vocal leveler's lane.
#[must_use]
pub fn lane_shown(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
) -> bool {
    Tracks::ext_state(daw, project.clone(), TrackRef::Guid(track.to_owned()))
        .iter()
        .any(|e| e.key == LANE_SHOWN && e.value == "1")
}

/// Show or hide the vocal leveler's lane under `track`.
pub fn show_lane(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    shown: bool,
) {
    let _ = Tracks::set_ext_state(
        daw,
        project.clone(),
        TrackRef::Guid(track.to_owned()),
        LANE_SHOWN,
        if shown { "1" } else { "" },
    );
}

/// Whether any of `track`'s items has lanes kept — not the key that
/// expands the leveler, which shares their prefix.
#[must_use]
pub fn has_lanes(daw: &daw::standalone::Standalone, project: &ProjectContext, track: &str) -> bool {
    Tracks::ext_state(daw, project.clone(), TrackRef::Guid(track.to_owned()))
        .iter()
        .any(|e| e.key.starts_with(LANES_KEY) && e.key != LANE_SHOWN && !e.value.is_empty())
}
