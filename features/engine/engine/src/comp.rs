//! Swipe comping: a stretch of one lane put onto the comp lane.
//!
//! REAPER 7 comps on fixed lanes: a track's takes each on a lane, one lane
//! the comp (its comping lane), and a stretch of a take swiped onto it.
//! What the comp held there is cut away, the take's audio over exactly
//! that stretch copied on, meeting the comp either side with a short
//! crossfade, and a comp area (`LINKEDLANE`) says where it came from — so
//! the take's lane can show what of it is in the comp.
//!
//! On a folder, every track under it with lanes at once: the same stretch
//! of the same lane, by name. A kit's mics are one performance, and comped
//! apart they fall out of phase with each other.
//!
//! On the engine, so the window's swipe, the Action List and a test are
//! one path ([`SWIPE_COMP`]); the window reads the song back after.

pub use crate::shared_actions::{SHOW_LANES, show_lanes};
use crate::shared_actions::{ShowArg, ShowOut};
use daw::service::{ItemRef, Items as _, ProjectContext, TakeRef, Takes as _, TrackRef, Tracks};
use daw_proto::primitives::{Duration, PositionInSeconds};
use facet::Facet;

/// The crossfade a swiped piece meets the comp with, seconds — the one the
/// window's own comp ([`crate::arrange_edit::Editor::comp`]) makes.
pub const JOIN: f64 = 0.005;

/// Shorter than this is no stretch at all, seconds.
pub const GRAIN: f64 = 1e-4;

/// The comp lane of `track`: the engine's comping lane (REAPER's
/// `LANEREC`), else the one the window comps onto
/// ([`crate::stretch::comp_lane`] — `C1`, `COMP`, else what plays).
#[must_use]
pub fn comp_lane_of(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &daw_proto::Track,
) -> Option<u32> {
    daw.comping(project.clone(), TrackRef::Guid(track.guid.clone()))
        .ok()
        .and_then(|c| c.comp_lane)
        .filter(|&lane| lane < track.lane_count)
        .or_else(|| crate::stretch::comp_lane(track))
}

/// Comp a lane over a range onto the comp lane, as one undo step.
///
/// The lane called `lane` (its name, or its number from 1) over
/// `from..to` seconds onto the comp lane of each of `tracks` — of a folder,
/// of every track under it with lanes — as one undo step. What it did,
/// track by track.
///
/// More than one track: a track and its media-edit group, swiped as one.
///
/// # Errors
///
/// An empty or backwards stretch; a track that is not there; none of them
/// with lanes; none with that lane apart from its comp; the engine
/// refusing a write (the step made so far is one undo).
pub fn swipe(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    tracks: &[String],
    lane: &str,
    (from, to): (f64, f64),
) -> Result<Vec<String>, String> {
    if !(from.is_finite() && to.is_finite() && to - from > GRAIN) {
        return Err(format!("nothing to comp between {from} and {to} s"));
    }
    let all = Tracks::all(daw, project.clone());
    let mut targets: Vec<&daw_proto::Track> = Vec::new();
    let mut named = Vec::new();
    for guid in tracks {
        let at = all
            .iter()
            .find(|t| &t.guid == guid)
            .ok_or_else(|| format!("no track {guid}"))?;
        named.push(at.name.as_str());
        for under in crate::lanes::shown_on(&all, at) {
            if !targets.iter().any(|t| t.guid == under.guid) {
                targets.push(under);
            }
        }
    }
    let named = named.join(", ");
    if targets.is_empty() {
        return Err(format!(
            "{named}: no lanes to comp from, nor any track under it"
        ));
    }
    // Each track's lane by that name, and its comp: a track without the
    // lane, or whose comp it is, is left.
    let plans: Vec<(&daw_proto::Track, u32, u32)> = targets
        .into_iter()
        .filter_map(|t| {
            let source = crate::lanes::lane_called(t, lane)?;
            let comp = comp_lane_of(daw, project, t).filter(|&c| c != source)?;
            Some((t, source, comp))
        })
        .collect();
    if plans.is_empty() {
        return Err(format!(
            "no track of {named} has a lane {lane} apart from its comp"
        ));
    }
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        all.iter().map(|t| (t.guid.as_str(), t)).collect();
    let label = format!("Comp from lane {lane}");
    daw::service::Projects::begin_undo_block(daw, project.clone(), &label);
    let mut said = Vec::new();
    let mut refused = None;
    for (target, source, comp) in plans {
        let path = crate::gain_stage::path_of(target, &by_guid).join("/");
        match comp_track(daw, project, target, (source, comp), (from, to)) {
            Ok(pieces) => said.push(format!(
                "{path}: lane {} over {from:.3}–{to:.3} s onto lane {} ({pieces} pieces)",
                crate::lanes::called(target, source),
                crate::lanes::called(target, comp),
            )),
            Err(e) => {
                refused = Some(format!("{path}: {e}"));
                break;
            }
        }
    }
    daw::service::Projects::end_undo_block(daw, project.clone(), &label, None);
    refused.map_or(Ok(said), Err)
}

/// [`swipe`] with each track's own source lane: `plans` is `(track GUID,
/// lane index)` — a take found on each mic by its recording, not by a
/// lane name the mics need not share. A track whose lane is its comp is
/// left. In the caller's undo step (`label` names the comp).
///
/// # Errors
///
/// Nothing to comp, a track gone, or the engine refusing an edit.
pub fn swipe_lanes(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    plans: &[(String, u32)],
    (from, to): (f64, f64),
    label: &str,
) -> Result<usize, String> {
    if !(from.is_finite() && to.is_finite() && to - from > GRAIN) {
        return Err(format!("nothing to comp between {from} and {to} s"));
    }
    let all = Tracks::all(daw, project.clone());
    let plans: Vec<(&daw_proto::Track, u32, u32)> = plans
        .iter()
        .filter_map(|(guid, source)| {
            let t = all.iter().find(|t| &t.guid == guid)?;
            let comp = comp_lane_of(daw, project, t).filter(|c| c != source)?;
            Some((t, *source, comp))
        })
        .collect();
    if plans.is_empty() {
        return Err("no mic has this take on a lane of its own".to_owned());
    }
    let _ = label;
    let mut done = 0usize;
    for (target, source, comp) in plans {
        comp_track(daw, project, target, (source, comp), (from, to))
            .map_err(|e| format!("{}: {e}", target.name))?;
        done = done.saturating_add(1);
    }
    Ok(done)
}

/// Each of `tracks` with `COMP` and `EDIT` lanes: its `EDIT` lane over
/// `from..to` made its `COMP` lane there — the edit taken off, the comp as
/// recorded — straight on the engine, as a swipe is. How many tracks.
///
/// # Errors
///
/// The engine refusing an edit.
pub fn edit_from_comp(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    tracks: &[String],
    (from, to): (f64, f64),
) -> Result<usize, String> {
    use crate::drum_lanes::{COMP, EDIT, lane_named};
    let all = Tracks::all(daw, project.clone());
    let mut done = 0usize;
    for t in all.iter().filter(|t| tracks.contains(&t.guid)) {
        let (Some(comp), Some(edit)) = (lane_named(t, COMP), lane_named(t, EDIT)) else {
            continue;
        };
        comp_track(daw, project, t, (comp, edit), (from, to))
            .map_err(|e| format!("{}: {e}", t.name))?;
        done = done.saturating_add(1);
    }
    Ok(done)
}

/// Each of `tracks` silent over `from..to`: what its `COMP` and `EDIT`
/// lanes hold there taken away, what they hold either side kept. How many
/// items it cut.
///
/// # Errors
///
/// The engine refusing an edit.
pub fn silence(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    tracks: &[String],
    (from, to): (f64, f64),
) -> Result<usize, daw_proto::DawError> {
    use crate::drum_lanes::{EDIT, lane_named};
    let all = Tracks::all(daw, project.clone());
    let mut cut = 0_usize;
    for t in all.iter().filter(|t| tracks.contains(&t.guid)) {
        let lanes: Vec<u32> = comp_lane_of(daw, project, t)
            .into_iter()
            .chain(lane_named(t, EDIT))
            .collect();
        for item in daw
            .get_items(project.clone(), TrackRef::Guid(t.guid.clone()))
            .iter()
            .filter(|i| lanes.contains(&i.fixed_lane.unwrap_or(0)))
            .filter(|i| {
                let (start, end) = span(i);
                end > from + GRAIN && start < to - GRAIN
            })
        {
            clear(daw, project, item, (from, to))?;
            cut = cut.saturating_add(1);
        }
    }
    Ok(cut)
}

/// [`swipe`] on one track: lane `source` over `from..to` onto lane `comp`.
/// How many pieces it copied.
fn comp_track(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &daw_proto::Track,
    (source, comp): (u32, u32),
    (from, to): (f64, f64),
) -> Result<usize, daw_proto::DawError> {
    let at = TrackRef::Guid(track.guid.clone());
    let items = daw.get_items(project.clone(), at.clone());
    let on = |item: &&daw_proto::Item, lane: u32| {
        let (start, end) = span(item);
        item.fixed_lane.unwrap_or(0) == lane && end > from + GRAIN && start < to - GRAIN
    };
    // The comp cleared over the stretch; what it held either side stays.
    for item in items.iter().filter(|i| on(i, comp)) {
        clear(daw, project, item, (from, to))?;
    }
    // The lane's audio over exactly the stretch, onto the comp.
    let mut pieces = 0_usize;
    for item in items.iter().filter(|i| on(i, source)) {
        let (start, end) = span(item);
        let copy = daw
            .duplicate_item(project.clone(), ItemRef::Guid(item.guid.clone()))
            .ok_or_else(|| daw_proto::DawError::not_found("Item", &item.guid))?;
        place(daw, project, &copy, item, (from.max(start), to.min(end)))?;
        daw.set_fixed_lane(project.clone(), ItemRef::Guid(copy), comp)?;
        pieces = pieces.saturating_add(1);
    }
    // Where it came from, for the take's lane to show.
    let areas = daw.comping(project.clone(), at.clone())?.areas;
    let area = daw_proto::CompArea {
        start: PositionInSeconds::from_seconds(from),
        end: PositionInSeconds::from_seconds(to),
        source_lane: source,
        comp_lane: comp,
        fade_in: Duration::from_seconds(JOIN),
        fade_out: Duration::from_seconds(JOIN),
    };
    daw.set_comp_areas(project.clone(), at, with_area(&areas, area))?;
    Ok(pieces)
}

/// An item's start and end, seconds.
fn span(item: &daw_proto::Item) -> (f64, f64) {
    let start = item.position.as_seconds();
    (start, start + item.length.as_seconds())
}

/// What of the comp item `item` lies in `from..to` taken away: its ends
/// either side kept — the right one playing on from where it was cut, as
/// a split does — with no fade of their own at the cut, the swiped piece's
/// crossfade covering it; wholly inside, it goes.
pub fn clear(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    item: &daw_proto::Item,
    (from, to): (f64, f64),
) -> Result<(), daw_proto::DawError> {
    let (start, end) = span(item);
    let at = ItemRef::Guid(item.guid.clone());
    let left = start < from - GRAIN;
    let right = end > to + GRAIN;
    if right {
        let piece = if left {
            ItemRef::Guid(
                daw.duplicate_item(project.clone(), at.clone())
                    .ok_or_else(|| daw_proto::DawError::not_found("Item", &item.guid))?,
            )
        } else {
            at.clone()
        };
        daw.set_position(
            project.clone(),
            piece.clone(),
            PositionInSeconds::from_seconds(to),
        )?;
        daw.set_length(
            project.clone(),
            piece.clone(),
            Duration::from_seconds(end - to),
        )?;
        slip(
            daw,
            project,
            &piece,
            item.start_offset.as_seconds() + (to - start),
        )?;
        daw.set_fade_in(
            project.clone(),
            piece,
            Duration::from_seconds(0.0),
            item.fade_in_shape,
        )?;
    }
    if left {
        daw.set_length(
            project.clone(),
            at.clone(),
            Duration::from_seconds(from - start),
        )?;
        daw.set_fade_out(
            project.clone(),
            at,
            Duration::from_seconds(0.0),
            item.fade_out_shape,
        )?;
    } else if !right {
        daw.delete_item(project.clone(), at)?;
    }
    Ok(())
}

/// The copy `copy` of `source` cut to `a..b`: there, playing what `source`
/// plays there, with the short crossfade either end.
fn place(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    copy: &str,
    source: &daw_proto::Item,
    (a, b): (f64, f64),
) -> Result<(), daw_proto::DawError> {
    let at = ItemRef::Guid(copy.to_owned());
    let (start, _) = span(source);
    daw.set_position(
        project.clone(),
        at.clone(),
        PositionInSeconds::from_seconds(a),
    )?;
    slip(
        daw,
        project,
        &at,
        source.start_offset.as_seconds() + (a - start),
    )?;
    daw.set_length(project.clone(), at.clone(), Duration::from_seconds(b - a))?;
    daw.set_fade_in(
        project.clone(),
        at.clone(),
        Duration::from_seconds(JOIN),
        source.fade_in_shape,
    )?;
    daw.set_fade_out(
        project.clone(),
        at,
        Duration::from_seconds(JOIN),
        source.fade_out_shape,
    )
}

/// `item`'s active take to start `offset` seconds into its source — an
/// item with no take (an empty one) has nothing to slip.
fn slip(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    item: &ItemRef,
    offset: f64,
) -> Result<(), daw_proto::DawError> {
    if daw.get_active_take(project.clone(), item.clone()).is_none() {
        return Ok(());
    }
    daw.set_start_offset(
        project.clone(),
        item.clone(),
        TakeRef::Active,
        Duration::from_seconds(offset),
    )
}

/// `areas` with `new` in: what areas of its comp lane it covers cut away
/// (their parts either side kept), the other comps' left, in order of
/// comp lane and start.
#[must_use]
pub fn with_area(
    areas: &[daw_proto::CompArea],
    new: daw_proto::CompArea,
) -> Vec<daw_proto::CompArea> {
    let (from, to) = (new.start.as_seconds(), new.end.as_seconds());
    let mut out: Vec<daw_proto::CompArea> = Vec::with_capacity(areas.len().saturating_add(2));
    for area in areas {
        let (start, end) = (area.start.as_seconds(), area.end.as_seconds());
        if area.comp_lane != new.comp_lane || end <= from + GRAIN || start >= to - GRAIN {
            out.push(area.clone());
            continue;
        }
        if start < from - GRAIN {
            out.push(daw_proto::CompArea {
                end: new.start,
                ..area.clone()
            });
        }
        if end > to + GRAIN {
            out.push(daw_proto::CompArea {
                start: new.end,
                ..area.clone()
            });
        }
    }
    out.push(new);
    out.sort_by(|a, b| {
        a.comp_lane
            .cmp(&b.comp_lane)
            .then(a.start.as_seconds().total_cmp(&b.start.as_seconds()))
    });
    out
}

/// The comp areas of the comp lane `comp` taken from `lane`: what of that
/// lane is in the comp, `(start, end)` seconds — what a take's lane marks.
#[must_use]
pub fn taken_from(areas: &[daw_proto::CompArea], comp: u32, lane: u32) -> Vec<(f64, f64)> {
    areas
        .iter()
        .filter(|a| a.comp_lane == comp && a.source_lane == lane)
        .map(|a| (a.start.as_seconds(), a.end.as_seconds()))
        .collect()
}

pub static SWIPE_COMP: architect::action::ActionMeta = crate::actions::meta(
    "TRACK_COMP_FROM_LANE",
    "track_comp_from_lane",
    "Comp from a lane",
    "Swipe comp: lane `lane` (its name — `2`, `C1` — or its number from 1) of `track` (a path, name or GUID; `with`, more tracks swiped with it, its media-edit group) over `start`..`end` seconds put onto the track's comp lane (its comping lane, else the lane named C1 or COMP, else the one that plays): what the comp held there cut away, the lane's audio over exactly that stretch copied on with a 5 ms crossfade either end, what it held either side left, and a comp area recorded. On a folder (`Drums`), every track under it with lanes at once, the same stretch of the same lane. Says what it did, track by track. One undo step.",
    "Edit",
    "",
);

pub static LANE_STEP: architect::action::ActionMeta = crate::actions::meta(
    "TRACK_LANE_STEP",
    "track_lane_step",
    "Play the next lane",
    "Play the next (`by` 1) or previous (`by` -1) lane of `track` (a path, name or GUID), round the ends — the track panel's lane arrows. The lane is chosen by name, so on a folder (`Drums`) every track under it with lanes plays the same lane, and while grouping is on so does each one's media-edit group. Says the lane each track plays now.",
    "Edit",
    "",
);

/// Which way to step whose lanes.
#[derive(Facet)]
struct LaneStepArg {
    track: String,
    /// 1 the next lane, -1 the previous.
    by: i32,
}

/// What each track plays now.
#[derive(Facet)]
pub struct LaneStepOut {
    /// The lane's name, as the panel shows it.
    pub lane: String,
    /// Each track stepped and the play mask it has now (bit `n`, lane `n`).
    pub tracks: Vec<LanePlays>,
}

/// One track's play mask.
#[derive(Facet)]
pub struct LanePlays {
    pub guid: String,
    pub mask: u64,
}

/// Step the lane `guid` plays (and what it speaks for) forward (`next`) or
/// back, by name across every track the step reaches: the lane's name and
/// each track's new mask.
fn step_lane(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    guid: &str,
    next: bool,
) -> Result<LaneStepOut, String> {
    let all = Tracks::all(daw, project.clone());
    let items_of = |g: &str| daw.get_items(project.clone(), TrackRef::Guid(g.to_owned()));
    let out = lane_step(&all, items_of, guid, next)?;
    for t in &out.tracks {
        Tracks::set_lane_play_mask(daw, project.clone(), TrackRef::Guid(t.guid.clone()), t.mask)
            .map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// `TRACK_LANE_STEP`'s decision over a song's tracks (`all`, each track's
/// items from `items_of`): the lane `guid` steps to and each track's mask
/// for it, nothing set.
///
/// # Errors
/// No such track, or one lane to play.
pub fn lane_step(
    all: &[daw_proto::Track],
    items_of: impl Fn(&str) -> Vec<daw_proto::Item>,
    guid: &str,
    next: bool,
) -> Result<LaneStepOut, String> {
    let track = all
        .iter()
        .find(|t| t.guid == guid)
        .ok_or("the track went")?;
    let shown = crate::lanes::shown_on(all, track);
    let speaks: Vec<&daw_proto::Track> = if shown.is_empty() { vec![track] } else { shown };
    let first = speaks.first().copied().ok_or("no lanes to step")?;
    let items = items_of(&first.guid);
    let mask = crate::lanes::stepped(first, &items, next)
        .ok_or_else(|| format!("{} has one lane to play", first.name))?;
    let lane = crate::lanes::label(&daw_proto::Track {
        lane_play_mask: mask,
        ..first.clone()
    });
    let mut targets: Vec<String> = Vec::new();
    for t in &speaks {
        for g in std::iter::once(t.guid.clone()).chain(crate::lanes::grouped_tracks(all, &t.guid)) {
            if !targets.contains(&g) {
                targets.push(g);
            }
        }
    }
    let mut tracks = Vec::new();
    for g in targets {
        let Some(t) = all.iter().find(|t| t.guid == g) else {
            continue;
        };
        let Some(own) = crate::lanes::mask_named(t, &lane) else {
            continue;
        };
        tracks.push(LanePlays { guid: g, mask: own });
    }
    Ok(LaneStepOut { lane, tracks })
}

/// What to comp from, where.
#[derive(Facet)]
struct SwipeArg {
    track: String,
    /// More tracks swiped with it (its media-edit group), each as `track`.
    #[facet(default)]
    with: Vec<String>,
    lane: String,
    start: f64,
    end: f64,
}

/// What a comp did.
#[derive(Facet)]
struct SwipeOut {
    /// `path: lane N over a–b s onto lane COMP (n pieces)`, each track.
    tracks: Vec<String>,
}

pub fn register(
    list: &architect::action::ActionList,
) -> Result<(), architect::action::ActionError> {
    use architect::action::Effect;
    list.register(&SWIPE_COMP, Effect::Mutating, |a: SwipeArg| {
        let (daw, project) = crate::actions::song()?;
        let guids = std::iter::once(&a.track)
            .chain(&a.with)
            .map(|t| crate::actions::resolve(daw, &project, t))
            .collect::<Result<Vec<String>, String>>()?;
        let tracks = swipe(daw, &project, &guids, &a.lane, (a.start, a.end))?;
        // A swipe cuts and copies items, nothing else a window draws (the
        // comp area it records is the engine's): the windows read back the
        // items the journal names, not the whole song — a second and a
        // half on Unbreakable.
        crate::song_model::request_resync_items();
        crate::actions::list().changed("lanes", "a lane was comped onto the comp");
        Ok(SwipeOut { tracks })
    })?;
    list.register(&LANE_STEP, Effect::Mutating, |a: LaneStepArg| {
        let (daw, project) = crate::actions::song()?;
        let guid = crate::actions::resolve(daw, &project, &a.track)?;
        let out = step_lane(daw, &project, &guid, a.by >= 0)?;
        for t in &out.tracks {
            crate::actions::list().changed("lanes", &t.guid);
        }
        Ok(out)
    })?;
    list.register(&SHOW_LANES, Effect::Mutating, |a: ShowArg| {
        // Told by the engine which tracks it opened: set here as settled.
        if let Some(tracks) = a.tracks.filter(|t| !t.is_empty()) {
            let shown = crate::lanes::set_expanded(&tracks, a.show);
            return Ok(ShowOut { shown, tracks });
        }
        let (daw, project) = crate::actions::song()?;
        let all = Tracks::all(daw, project.clone());
        let guid = match a.track.as_deref() {
            Some(track) => crate::actions::resolve(daw, &project, track)?,
            None => all
                .iter()
                .find(|t| t.selected)
                .map(|t| t.guid.clone())
                .ok_or("no track given, and none selected")?,
        };
        let (shown, tracks) = show_lanes(&all, &guid, a.show)?;
        // The lanes a row shows are the window's: run in the engine, the
        // window is asked too, told the state settled here so the two
        // agree whichever ran it first.
        crate::actions::also_in_the_window(
            SHOW_LANES.id,
            serde_json::json!({ "track": guid, "show": shown, "tracks": tracks }).to_string(),
        );
        Ok(ShowOut { shown, tracks })
    })?;
    Ok(())
}
