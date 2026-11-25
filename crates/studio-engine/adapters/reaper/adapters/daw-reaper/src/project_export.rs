//! Domain-level export from REAPER back to Pro Tools.
//!
//! This is the in-memory inverse of the playlist→fixed-lane mapping in
//! `crate::project_import`. The forward path turns a Pro Tools `Track` with
//! more than one playlist (active playlist in `Track.regions`, the rest in
//! `Track.alternate_playlists`) into a single REAPER track in fixed-lane mode:
//! items carry `lane = Some(idx)` (lane 0 = active, 1..N = alternates) and the
//! track gets `fixed_lanes` / `lane_names` / `lane_record` populated.
//!
//! Here we walk that mapping in reverse: a REAPER `Track` (plus its `Item`s)
//! becomes a Pro Tools `Track` whose `regions` are the lane-0 items and whose
//! `alternate_playlists` are lanes 1..N (in lane order).
//!
//! Scope: this stops at the Pro Tools domain structs. Serializing those back to
//! `.ptx` bytes is a separate, still-unresolved reverse-engineering task and is
//! intentionally NOT attempted here.
//!
//! Cross-domain mapping lives in `daw-reaper` (not in `dawfile-protools`)
//! because `daw-reaper` already depends on both dawfile crates, and
//! `dawfile-protools` must not gain a dependency on `dawfile-reaper`.

// PT domain types. `dawfile_protools` re-exports its `types` module at the
// crate root (see the importer's use of `protools::TrackRegion`), so
// these resolve to the same structs documented in `dawfile-protools/src/types.rs`.
use dawfile_reaper::types::item::Item;
use dawfile_reaper::types::track::Track as ReaperTrack;
use protools::{Playlist, Track as PtTrack, TrackKind, TrackRegion};

/// The PT `TrackRegion` time unit is samples on the timeline; the forward path
/// converts PT samples → REAPER seconds via the session sample rate. Reversing
/// position/length therefore needs a sample rate. PT sessions are overwhelmingly
/// 48 kHz, and the importer itself defaults to 48000 when reading; we mirror
/// that as the fallback so a caller that doesn't track the rate still produces
/// sane sample positions.
const DEFAULT_SAMPLE_RATE: f64 = 48_000.0;

/// Map a REAPER `Item`'s timeline placement to a Pro Tools `TrackRegion`.
///
/// Only the placement-level fields carry across cleanly:
/// - `start_pos`  ← `item.position` (seconds → samples at `sample_rate`)
///
/// REAPER items reference a take/source rather than a session-global region
/// table, so there is no REAPER analogue for PT's `region_index` (an index into
/// `session.audio_regions` / `session.midi_regions`). We emit `region_index = 0`
/// as a placeholder; a future PT writer that owns a region table would have to
/// allocate/resolve a real index. Likewise REAPER has no field corresponding to
/// PT's per-clip flags / MIDI source-window bounds, so those take their neutral
/// defaults (mirroring `parse::tracks`' construction).
fn item_to_track_region(item: &Item, sample_rate: f64) -> TrackRegion {
    let start_pos = (item.position * sample_rate).round() as u64;
    TrackRegion {
        // No REAPER source for a session-global region index; the PT domain
        // requires one, so emit 0 as a placeholder (a real .ptx writer would
        // resolve this against an allocated region table).
        region_index: 0,
        start_pos,
        // Neutral per-clip defaults — REAPER carries no matching source field.
        clip_flag_53: false,
        clip_muted: item.mute.as_ref().map(|m| m.muted).unwrap_or(false),
        clip_color: None,
        // Audio-style bounds (full take) — see `parse::tracks` which sets these
        // for audio placements. MIDI source-window trimming is not recoverable
        // from a flattened REAPER item.
        clip_lo_ticks: 0,
        note_trim_ticks: u64::MAX,
    }
}

/// Convert a REAPER track (with its items) into a Pro Tools track carrying
/// alternate playlists — the inverse of `crate::project_import`'s
/// playlist→fixed-lane conversion.
///
/// Items are grouped by `Item::lane`:
/// - `None` or `Some(0)` → the **active** playlist (PT `Track.regions`)
/// - `Some(i)` for `i >= 1` → `alternate_playlists[i - 1]` (in lane order)
///
/// Playlist names come from `track.lane_names` when present (lane 0 = active
/// name, lanes 1..N = alternate names). Otherwise they are synthesized: the
/// active playlist takes the bare track name and alternate `k` (1-based) takes
/// the PT `.NN` suffix convention (`"Kick.01"`, `"Kick.02"`, …), matching what
/// the forward parser strips in `dawfile-protools`.
///
/// When the track is NOT in fixed-lane mode (`fixed_lanes.is_none()` and no item
/// carries a lane >= 1), the result is a single-playlist PT track with an empty
/// `alternate_playlists` — the inverse of the single-playlist passthrough.
///
/// `sample_rate` is the PT session sample rate used to convert REAPER seconds
/// back to PT samples for region positions.
pub fn reaper_track_to_pt_playlists_at_rate(
    track: &ReaperTrack,
    items: &[Item],
    sample_rate: f64,
) -> PtTrack {
    // Determine the highest alternate lane index actually present, considering
    // BOTH explicit item lanes and the declared lane_names count. A track can be
    // flagged fixed-lane (lane_names present) even if some lanes are empty.
    let max_item_lane = items
        .iter()
        .filter_map(|it| it.lane)
        .filter(|&l| l >= 1)
        .max()
        .unwrap_or(0);
    let declared_lane_count = track
        .lane_names
        .as_ref()
        .map(|ln| ln.lane_count.max(0))
        .unwrap_or(0);
    let is_fixed_lane = track.fixed_lanes.is_some() || max_item_lane >= 1;

    // Total lanes = max(declared count, highest used lane + 1). Always at least
    // 1 (the active playlist exists even with no items).
    let lane_count: usize = if is_fixed_lane {
        let from_items = (max_item_lane as usize) + 1;
        from_items.max(declared_lane_count as usize).max(1)
    } else {
        1
    };

    // Bucket items per lane. Lane 0 absorbs `None` and `Some(0)`.
    let mut lanes: Vec<Vec<TrackRegion>> = vec![Vec::new(); lane_count];
    for item in items {
        let lane = item.lane.unwrap_or(0);
        let idx = if lane < 0 { 0 } else { lane as usize };
        // Defensive: an item lane beyond lane_count would otherwise panic.
        if idx < lanes.len() {
            lanes[idx].push(item_to_track_region(item, sample_rate));
        }
    }

    // Resolve a playlist name for lane `i`. Prefer the explicit lane name; fall
    // back to the synthesized convention (bare name for active, `.NN` for alts).
    let lane_name = |i: usize| -> String {
        if let Some(ln) = track.lane_names.as_ref()
            && let Some(name) = ln.lane_names.get(i)
            && !name.is_empty()
        {
            return name.clone();
        }
        if i == 0 {
            track.name.clone()
        } else {
            // PT alternate suffix: `.01`, `.02`, … (1-based).
            format!("{}.{:02}", track.name, i)
        }
    };

    let active_name = lane_name(0);

    // Lane 0 → active playlist.
    let active_regions = lanes.first().cloned().unwrap_or_default();

    // Lanes 1..N → alternate playlists in lane order.
    let alternate_playlists: Vec<Playlist> = (1..lane_count)
        .map(|i| Playlist {
            name: lane_name(i),
            regions: lanes.get(i).cloned().unwrap_or_default(),
        })
        .collect();

    PtTrack {
        name: track.name.clone(),
        // The forward path only handles audio fixed lanes; MIDI playlist round-
        // trip is out of scope, so classify as Audio (the only fixed-lane case
        // the importer produces).
        kind: TrackKind::Audio,
        index: 0,
        playlist_name: active_name,
        regions: active_regions,
        // Fades / mix state / automation have no part in the playlist mapping
        // and are left at the same neutral defaults the PT parser starts from.
        fades: Vec::new(),
        volume_centibel: 0,
        mute: false,
        solo: false,
        solo_defeat: false,
        inactive: false,
        pan: 0,
        alternate_playlists,
        output: String::new(),
        color_byte: 0,
        height_px: 0,
        volume_automation: Vec::new(),
        mute_automation: Vec::new(),
        pan_automation: Vec::new(),
        is_folder: false,
        folder_depth: 0,
        display_order: u32::MAX,
        comment: String::new(),
        is_master: false,
    }
}

/// Convenience wrapper over `reaper_track_to_pt_playlists_at_rate` using the
/// default Pro Tools sample rate (48 kHz), matching the importer's default.
///
/// In the REAPER model items hang off the track directly (`Track.items`), so a
/// typical call site is `reaper_track_to_pt_playlists(&track, &track.items)`.
pub fn reaper_track_to_pt_playlists(track: &ReaperTrack, items: &[Item]) -> PtTrack {
    reaper_track_to_pt_playlists_at_rate(track, items, DEFAULT_SAMPLE_RATE)
}
