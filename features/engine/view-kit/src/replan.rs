//! The rows' project brought up to the song after an edit that changed
//! only items: [`patch_planned`].
//!
//! Planning the rows ([`crate::song_model::Planner::plan`]) copies the
//! whole song and folds every shut folder again — tens of milliseconds on
//! a sliced kit, for an edit that moved three slices. The rows themselves
//! are planned from the tracks alone, so when only items changed they are
//! as they were: the project they are drawn from takes the changed tracks'
//! items, and the shut folders over them are folded again — those only.
//!
//! ```ignore
//! match patch_planned(&mut planned, &raw, &rows, Folding::current()) {
//!     Patched::Same => {}                 // nothing to draw again
//!     Patched::Items => recut(),          // the rows as they were
//!     Patched::Replan => replan(),        // more than items changed
//! }
//! ```

use daw_proto::Track;
use daw_ui::studio::project::Project;
use std::collections::{BTreeMap, BTreeSet};

use rustc_hash::{FxHashMap, FxHashSet};

/// What [`patch_planned`] made of the change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Patched {
    /// Nothing the rows are drawn from changed.
    Same,
    /// Items changed, and the planned project has them: the rows stand.
    Items,
    /// More than items changed (the tracks): plan the rows again.
    Replan,
}

/// How shut folders were folded when the rows were planned.
#[derive(Clone, Copy)]
pub struct Folding<'a> {
    /// Whether shut folders show what is under them at all
    /// (`Settings::folded_takes`).
    pub on: bool,
    /// Whether they show their cuts, or the seams joined
    /// ([`crate::folds::cuts_shown`]).
    pub cuts: bool,
    /// Whether a shut folder (asked with every track) is folded lane by
    /// lane ([`crate::lanes::folder_view`]).
    pub laned: &'a dyn Fn(&[Track], &Track) -> bool,
}

impl Folding<'static> {
    /// As the window's planner folds them now (`song_model::plan_rows_with`).
    #[must_use]
    pub fn current() -> Self {
        Self {
            on: std::env::var("FTS_BLITZ_FOLDED_TAKES").as_deref() != Ok("0"),
            cuts: crate::folds::cuts_shown(),
            laned: &|tracks, folder| crate::lanes::folder_view(tracks, folder).is_some(),
        }
    }
}

/// `planned` — the project `rows` were planned with, from the song as it
/// was — brought up to `raw`, the song now, when the tracks are as they
/// were: each track whose items (or their names) changed takes them from
/// `raw`, each shut folder over one is folded again `folding`'s way, and
/// the ruler's marks, tempo and length are `raw`'s. What
/// [`crate::song_model::Planner::plan`] would make of `raw`, without
/// planning the rows or folding anything an edit did not touch.
pub fn patch_planned(
    planned: &mut Project,
    raw: &Project,
    rows: &[(Track, u32)],
    folding: Folding<'_>,
) -> Patched {
    if planned.tracks != raw.tracks {
        return Patched::Replan;
    }
    let dirty = changed_tracks(planned, raw);
    let ruler = take_ruler(planned, raw);
    if dirty.is_empty() {
        return if ruler { Patched::Items } else { Patched::Same };
    }
    let lanes = changed_lanes(planned, raw, &dirty);
    take_items(planned, raw, &dirty);
    if folding.on {
        refold(planned, &raw.tracks, rows, (&lanes, folding));
    }
    Patched::Items
}

/// Each of the `dirty` tracks and the lanes (`fixed_lane`) its items
/// changed on between `planned` and `raw`: none for a track whose items
/// only took another name.
fn changed_lanes<'r>(
    planned: &Project,
    raw: &'r Project,
    dirty: &FxHashSet<&'r str>,
) -> FxHashMap<&'r str, BTreeSet<u32>> {
    fn by_lane(items: Option<&Vec<daw_proto::Item>>) -> BTreeMap<u32, Vec<&daw_proto::Item>> {
        let mut lanes: BTreeMap<u32, Vec<&daw_proto::Item>> = BTreeMap::new();
        for item in items.into_iter().flatten() {
            lanes
                .entry(item.fixed_lane.unwrap_or(0))
                .or_default()
                .push(item);
        }
        lanes
    }
    dirty
        .iter()
        .map(|track| {
            let (was, now) = (
                by_lane(planned.items.get(*track)),
                by_lane(raw.items.get(*track)),
            );
            let lanes = was
                .keys()
                .chain(now.keys())
                .filter(|lane| was.get(lane) != now.get(lane))
                .copied()
                .collect();
            (*track, lanes)
        })
        .collect()
}

/// The tracks whose items differ between `planned` and `raw`, or whose
/// items' names or MIDI-ness do.
fn changed_tracks<'r>(planned: &Project, raw: &'r Project) -> FxHashSet<&'r str> {
    let mut dirty: FxHashSet<&'r str> = raw
        .items
        .iter()
        .filter(|(track, items)| planned.items.get(*track) != Some(*items))
        .map(|(track, _)| track.as_str())
        .collect();
    // Gone from the song: the track is in it still, its items are not.
    dirty.extend(
        planned
            .items
            .keys()
            .filter(|track| !raw.items.contains_key(*track))
            .filter_map(|track| raw.tracks.iter().find(|t| t.guid == **track))
            .map(|t| t.guid.as_str()),
    );
    if planned.names != raw.names || planned.midi != raw.midi {
        let renamed: FxHashSet<&str> = raw
            .names
            .iter()
            .filter(|(guid, name)| planned.names.get(*guid) != Some(*name))
            .map(|(guid, _)| guid.as_str())
            .chain(
                planned
                    .names
                    .keys()
                    .filter(|guid| !raw.names.contains_key(*guid))
                    .map(String::as_str),
            )
            .chain(
                raw.midi
                    .symmetric_difference(&planned.midi)
                    .map(String::as_str),
            )
            .collect();
        dirty.extend(
            raw.items
                .iter()
                .filter(|(_, items)| items.iter().any(|i| renamed.contains(i.guid.as_str())))
                .map(|(track, _)| track.as_str()),
        );
    }
    dirty
}

/// `raw`'s ruler — sections, markers, tempo — and length onto `planned`;
/// whether any of it changed.
fn take_ruler(planned: &mut Project, raw: &Project) -> bool {
    let changed = planned.sections != raw.sections
        || planned.markers != raw.markers
        || planned.tempo != raw.tempo
        || planned.bpm.to_bits() != raw.bpm.to_bits()
        || planned.length_secs.to_bits() != raw.length_secs.to_bits()
        || planned.item_count != raw.item_count;
    if changed {
        planned.sections.clone_from(&raw.sections);
        planned.markers.clone_from(&raw.markers);
        planned.tempo.clone_from(&raw.tempo);
        planned.bpm = raw.bpm;
        planned.length_secs = raw.length_secs;
        planned.item_count = raw.item_count;
    }
    changed
}

/// The `dirty` tracks' items, their names and MIDI-ness, from `raw`.
fn take_items(planned: &mut Project, raw: &Project, dirty: &FxHashSet<&str>) {
    for track in dirty {
        let gone = planned.items.remove(*track).unwrap_or_default();
        for item in &gone {
            planned.names.remove(&item.guid);
            planned.midi.remove(&item.guid);
        }
        let Some(items) = raw.items.get(*track) else {
            continue;
        };
        for item in items {
            if let Some(name) = raw.names.get(&item.guid) {
                planned.names.insert(item.guid.clone(), name.clone());
            }
            if raw.midi.contains(&item.guid) {
                planned.midi.insert(item.guid.clone());
            }
        }
        planned.items.insert((*track).to_owned(), items.clone());
    }
}

/// Every shut folder of `rows` over a changed track (`lanes`, each with
/// the lanes it changed on) folded again — and its seams joined, unless
/// the cuts show — as the planner folds it. A folder folded lane by lane
/// folds again only the lanes that changed.
fn refold(
    planned: &mut Project,
    tracks: &[Track],
    rows: &[(Track, u32)],
    (lanes, folding): (&FxHashMap<&str, BTreeSet<u32>>, Folding<'_>),
) {
    use daw_ui::studio::folded;
    let shown: Vec<Track> = rows.iter().map(|(track, _)| track.clone()).collect();
    for folder in folded::shut(tracks, &shown) {
        let changed: Vec<&BTreeSet<u32>> = folded::under(tracks, &folder.guid)
            .iter()
            .filter_map(|track| lanes.get(track.guid.as_str()))
            .collect();
        if changed.is_empty() {
            continue;
        }
        let laned = (folding.laned)(tracks, folder);
        let was = planned
            .folds
            .get(&folder.guid)
            .filter(|was| laned && was.items.iter().all(|item| item.fixed_lane.is_some()));
        let fold = match was {
            Some(was) => {
                let lanes: BTreeSet<u32> = changed.into_iter().flatten().copied().collect();
                folded::refold_lanes_of(planned, tracks, folder, (was, &lanes), !folding.cuts)
            }
            None => folded::fold_of(planned, tracks, folder, laned).map(|fold| {
                if folding.cuts {
                    fold
                } else {
                    folded::joined(folder, fold)
                }
            }),
        };
        match fold {
            Some(fold) => planned.folds.insert(folder.guid.clone(), fold),
            None => planned.folds.remove(&folder.guid),
        };
    }
}

#[cfg(test)]
mod tests;
