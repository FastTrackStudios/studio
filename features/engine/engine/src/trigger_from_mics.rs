//! A trigger made from the mics, where a take's trigger recorded nothing.
//!
//! A drum's `Trig` track keys its gate. On a take whose trigger for that
//! drum is dead — a pickup off, a cable out — the gate never opens, and a
//! comp that plays that take (Unbreakable's intro, its toms) silences the
//! drum. The hits are there all the same: [`crate::hits`] read them off the
//! drum's close mics, the very hits the kit's MIDI was made from, and wrote
//! each one as a marker on every recording of the piece, the dead trigger's
//! too. So the trigger is made from them: a file in the song's own `Media`
//! folder (never beside a recording another song keeps), a click at each
//! hit the mics heard, and the takes cut from the dead file play it
//! instead. The gate, its key and its sends stay as they are.
//!
//! A take may have no trigger at all: recorded in another session, its
//! `Trig` track holds nothing of it, and wherever the comp plays it the
//! gate is shut. There a trigger is made from the take's own mic hits too,
//! and the `Trig` track given an item for each of the mic's — on the comp
//! lane and on the lane edits are made on, slice for slice — that it has
//! none for: it plays where the mic plays, and is cut with the kit after.

use std::path::{Path, PathBuf};

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{Items as _, Takes as _, Tracks};
use daw_proto::{ItemRef, ProjectContext, TakeMarker, TakeRef, TrackRef};
use facet::Facet;

/// How sure a mic-only hit must be to become a click: below it, a hit the
/// detector doubted (bleed it could not refuse) would open the gate on
/// another drum.
pub const MIN_CONFIDENCE: f64 = 0.5;

/// A click's length, seconds: long enough for a gate's detector to see,
/// far shorter than any stroke.
const CLICK_SECS: f64 = 0.001;

/// A click's peak: well over any gate threshold fitted to a trigger, under
/// full scale.
const CLICK_PEAK: f32 = 0.9;

/// What a generated trigger's file is called after the one it stands for.
const SUFFIX: &str = " (from mics)";

/// Where `piece`'s hits are in a recording whose trigger heard none of
/// them: the mic-heard hits sure enough to key a gate, seconds into the
/// source, in order. `None` when the trigger heard any (it works here) or
/// the mics heard none (nothing to make).
#[must_use]
pub fn dead_trigger_hits(markers: &[TakeMarker], piece: &str) -> Option<Vec<f64>> {
    let mut heard_by_trigger = false;
    let mut at: Vec<f64> = Vec::new();
    for m in markers {
        let Some(hit) = m.hit.as_ref().filter(|h| h.piece == piece) else {
            continue;
        };
        heard_by_trigger |= hit.from_trigger;
        if hit.from_mic && !hit.bleed && hit.confidence >= MIN_CONFIDENCE {
            at.push(m.source_position_seconds);
        }
    }
    if heard_by_trigger || at.is_empty() {
        return None;
    }
    at.sort_by(f64::total_cmp);
    Some(at)
}

/// `frames` of silence at `rate` with a click at each of `at` (seconds): a
/// half-sine [`CLICK_SECS`] long, [`CLICK_PEAK`] high — a trigger's spike.
#[must_use]
pub fn clicks(at: &[f64], frames: usize, rate: u32) -> Vec<f32> {
    let mut out = vec![0.0_f32; frames];
    let width = ((CLICK_SECS * f64::from(rate)).round() as usize).max(2);
    for &t in at {
        let Some(start) = (t >= 0.0).then(|| (t * f64::from(rate)).round() as usize) else {
            continue;
        };
        for (i, s) in out.iter_mut().skip(start).take(width).enumerate() {
            let phase = std::f64::consts::PI * (i as f64 + 0.5) / width as f64;
            *s = CLICK_PEAK * phase.sin() as f32;
        }
    }
    out
}

/// The file a trigger made for `source` (a recording, as the project
/// names it) is written as: its stem and [`SUFFIX`].
#[must_use]
pub fn made_name(source: &str) -> String {
    let file = source.rsplit_once('/').map_or(source, |(_, f)| f);
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    format!("{stem}{SUFFIX}.wav")
}

/// Whether a hit is sure enough to key a gate: placed or checked by hand,
/// or heard by the mic, not bleed, and not doubted.
fn sure(hit: &daw_proto::HitMark) -> bool {
    hit.origin != daw_proto::MarkOrigin::Auto
        || (hit.from_mic && !hit.bleed && hit.confidence >= MIN_CONFIDENCE)
}

/// `piece`'s sure hits in `markers` (a mic recording's), seconds into the
/// source, in order: what a trigger made for a take that has none clicks
/// at.
#[must_use]
pub fn sure_hits(markers: &[TakeMarker], piece: &str) -> Vec<f64> {
    let mut at: Vec<f64> = markers
        .iter()
        .filter(|m| m.hit.as_ref().is_some_and(|h| h.piece == piece && sure(h)))
        .map(|m| m.source_position_seconds)
        .collect();
    at.sort_by(f64::total_cmp);
    at
}

/// An item's lane and where it plays: `(lane, from, to)`, seconds.
pub type Placed = (Option<u32>, f64, f64);

/// How much of a mic item the trigger's items on its lane must cover for
/// it to have a trigger there.
const COVERED: f64 = 0.9;

/// Which of `mic` (the items of a piece's mic) no item of `trig` (its
/// trigger's) on the same lane covers ([`COVERED`] of its length): where
/// the take that plays has no trigger.
#[must_use]
pub fn uncovered(mic: &[Placed], trig: &[Placed]) -> Vec<usize> {
    mic.iter()
        .enumerate()
        .filter(|&(_, &(lane, from, to))| {
            let length = to - from;
            let mut spans: Vec<(f64, f64)> = trig
                .iter()
                .filter(|t| t.0 == lane && t.2 > from && t.1 < to)
                .map(|t| (t.1.max(from), t.2.min(to)))
                .collect();
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            let covered: f64 = merged(&spans).iter().map(|(a, b)| b - a).sum();
            length > 0.0 && covered < COVERED * length
        })
        .map(|(i, _)| i)
        .collect()
}

/// How far apart two stretches may be and still be one, seconds: the
/// overlap and rounding between neighbouring slices.
const JOINED: f64 = 0.010;

/// `spans` (in order of start) joined where they meet or overlap.
#[must_use]
pub fn merged(spans: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    for &(from, to) in spans {
        match out.last_mut() {
            Some(last) if from <= last.1 + JOINED => last.1 = last.1.max(to),
            _ => out.push((from, to)),
        }
    }
    out
}

static FROM_MICS: ActionMeta = crate::actions::meta(
    "DRUMS_TRIGGERS_FROM_MICS",
    "drums_triggers_from_mics",
    "Triggers from the mics",
    "Where a take's trigger recorded nothing for its drum (none of the piece's hit markers on that recording were heard by the trigger, while its close mics heard some), make the trigger from the mics: a file in the song's own Media folder, `<name> (from mics).wav`, a 1 ms click at each hit the mics heard (confidence 0.5 or more, not bleed) — the hits the kit's MIDI was made from — and every take cut from the dead file plays it instead, its hit markers with it. The drum's gate is then keyed on the drum as played. Where a take has no trigger at all (recorded in another session: its piece's `Trig` track has no item where the comp plays it), the trigger is made from that take's own mic hits (sure ones, and every hit placed or checked by hand), and the `Trig` track given an item for each of the mic's it has none for — on the comp lane and the lane edits are made on, slice for slice — so it plays where the mic plays; `filled` lists them, each with the stretches the song plays that take without a trigger (`dry_run` lists them and makes nothing). Run after HITS_DETECT. `pieces` (all when left out); `dry_run` says what it would do. One undo step. Sets a mark on the song first, in its history (`Before DRUMS_TRIGGERS_FROM_MICS <date time>`, named in the result's `checkpoint`; SESSION_HISTORY_RESTORE `mark` goes back to it), and is refused if one cannot be made.",
    "Drums",
    "",
);

/// Which pieces, and whether only to say.
#[derive(Facet)]
struct FromMicsArg {
    #[facet(default)]
    pieces: Vec<String>,
    #[facet(default)]
    dry_run: bool,
}

/// One dead trigger made from the mics.
#[derive(Facet, Debug, Clone, PartialEq)]
struct Made {
    piece: String,
    /// The dead recording, and the one made for it, as the project names
    /// them.
    from: String,
    to: String,
    /// The clicks it holds, and the takes now playing it.
    clicks: u32,
    takes: u32,
}

/// A take with no trigger, given one made from its mic.
#[derive(Facet, Debug, Clone, PartialEq)]
struct Filled {
    piece: String,
    /// The mic recording the trigger is made from, and the file made.
    from: String,
    to: String,
    clicks: u32,
    /// Items made on the `Trig` track, comp and edit lanes alike.
    items: u32,
    /// Where the song plays that take with no trigger now:
    /// `72.300–91.000 s (bar 72 beat 1.00), 18.70 s`.
    stretches: Vec<String>,
}

/// What it did, or would.
#[derive(Facet)]
struct FromMicsOut {
    made: Vec<Made>,
    /// Takes with no trigger, given one from their mics.
    filled: Vec<Filled>,
    /// Recordings whose trigger works, or with nothing heard to make one
    /// from — `piece: file`.
    left: Vec<String>,
    /// The checkpoint forked before anything was made (`safety_net`);
    /// none for a dry run or with nothing to make.
    checkpoint: Option<String>,
}

/// A trigger recording as the takes cut from it name it, and those takes.
struct Recording {
    source: String,
    /// (item, take) of every take cut from it.
    takes: Vec<(String, String)>,
}

/// Every recording `track` plays from, with the takes cut from each.
fn recordings(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
) -> Vec<Recording> {
    let mut out: Vec<Recording> = Vec::new();
    for item in daw.get_items(project.clone(), TrackRef::Guid(track.to_owned())) {
        let Some(take) = daw.get_active_take(project.clone(), ItemRef::Guid(item.guid.clone()))
        else {
            continue;
        };
        let Some(source) = take.source_file_path.filter(|s| !s.contains(SUFFIX)) else {
            continue;
        };
        match out.iter_mut().find(|r| r.source == source) {
            Some(r) => r.takes.push((item.guid, take.guid)),
            None => out.push(Recording {
                source,
                takes: vec![(item.guid, take.guid)],
            }),
        }
    }
    out
}

/// Where a file called `name` goes in `media` without writing over one
/// there: `name`, else `<stem> 2.wav`, `<stem> 3.wav`, … — a file an edit
/// wrote is never overwritten, so an older version of the song (gone back
/// to through its history) still plays the one it was made with.
#[must_use]
pub fn fresh(media: &Path, name: &str) -> PathBuf {
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, "wav"));
    std::iter::once(media.join(name))
        .chain((2_u32..).map(|n| media.join(format!("{stem} {n}.{ext}"))))
        .find(|p| !p.exists())
        .unwrap_or_else(|| media.join(name))
}

/// Write the trigger for `hits` to `to` (a [`fresh`] path in the song's
/// own folder) for `disk` (the recording it stands for): its rate, its
/// length, one channel.
fn write_made(disk: &Path, to: &Path, hits: &[f64]) -> Result<(), String> {
    let pcm = fts_sample::mapped::PcmFile::open(disk).map_err(|e| e.to_string())?;
    let rate = pcm.sample_rate();
    let frames = pcm.frames();
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fts_sample::write_wav_f32(to, rate, &[clicks(hits, frames, rate)]).map_err(|e| e.to_string())
}

/// `DRUMS_TRIGGERS_FROM_MICS`: what [`make`] would do, then — when it
/// would make anything and this is not a dry run — a checkpoint, and it
/// done.
fn run(a: FromMicsArg) -> Result<FromMicsOut, String> {
    let plan = make(&FromMicsArg {
        pieces: a.pieces.clone(),
        dry_run: true,
    })?;
    if a.dry_run || (plan.made.is_empty() && plan.filled.is_empty()) {
        return Ok(plan);
    }
    let net = crate::safety_net::before(FROM_MICS.id, crate::safety_net::Reach::Song)?;
    let mut out = make(&a)?;
    out.checkpoint = net.name();
    Ok(out)
}

fn make(a: &FromMicsArg) -> Result<FromMicsOut, String> {
    let song = crate::song::Song::current()?;
    let (daw, project) = (song.daw(), song.context());
    let (pieces, _) = crate::hits::kit(daw, &project);
    let bay = daw.media_bay();
    // The song's own Media folder, beside its `.session`.
    let media = crate::project_file::path_of(song.id().as_str())
        .and_then(|p| p.parent().map(|d| d.join("Media")))
        .ok_or("the song has no file to keep a made trigger beside")?;
    let mut out = FromMicsOut {
        made: Vec::new(),
        filled: Vec::new(),
        left: Vec::new(),
        checkpoint: None,
    };
    let label = "Triggers from the mics";
    if !a.dry_run {
        daw::service::Projects::begin_undo_block(daw, project.clone(), label);
    }
    let done = (|| -> Result<(), String> {
        for piece in pieces.iter().filter(|p| {
            a.pieces.is_empty() || a.pieces.iter().any(|n| n.eq_ignore_ascii_case(&p.name))
        }) {
            for rec in recordings(daw, &project, &piece.trigger) {
                let Some((item, take)) = rec.takes.first() else {
                    continue;
                };
                let markers = daw.get_take_markers(
                    project.clone(),
                    ItemRef::Guid(item.clone()),
                    TakeRef::Guid(take.clone()),
                );
                let Some(hits) = dead_trigger_hits(&markers, &piece.name) else {
                    out.left.push(format!("{}: {}", piece.name, rec.source));
                    continue;
                };
                let to = fresh(&media, &made_name(&rec.source)).display().to_string();
                let made = Made {
                    piece: piece.name.clone(),
                    from: rec.source.clone(),
                    to: to.clone(),
                    clicks: u32::try_from(hits.len()).unwrap_or(u32::MAX),
                    takes: u32::try_from(rec.takes.len()).unwrap_or(u32::MAX),
                };
                if a.dry_run {
                    out.made.push(made);
                    continue;
                }
                let disk = bay
                    .resolve_file_path(&rec.source)
                    .ok_or_else(|| format!("{} is not on this machine", rec.source))?;
                write_made(&disk, Path::new(&to), &hits)?;
                bay.replace_in_project(
                    project.clone(),
                    &rec.source,
                    &to,
                    daw::standalone::media_bay::ReplaceScope::AllInstances,
                )
                .map_err(|e| e.to_string())?;
                // Its hits are where they were, in a file of the same
                // length: the markers go with it.
                let _ = daw.write_project(song.id().as_str(), |p| {
                    if let Some(marks) = p.take_markers.get(&rec.source).cloned() {
                        p.take_markers.insert(to.clone(), marks);
                    }
                });
                for (_, take) in &rec.takes {
                    daw::standalone::audio_engine::materialize::materialize_take_via_bay(
                        daw,
                        song.id().as_str(),
                        take,
                        &to,
                    )?;
                }
                out.made.push(made);
            }
        }
        for piece in pieces.iter().filter(|p| {
            a.pieces.is_empty() || a.pieces.iter().any(|n| n.eq_ignore_ascii_case(&p.name))
        }) {
            fill(&song, piece, &media, a.dry_run, &mut out)?;
        }
        Ok(())
    })();
    if !a.dry_run {
        daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
        crate::song_model::request_resync();
    }
    done?;
    Ok(out)
}

/// Give `piece`'s `Trig` track an item for each item of its first mic
/// that it has none for — on the comp lane and the lane edits are made on
/// — playing a trigger made from that mic recording's hits (`dry`: only
/// say where). What it did goes into `out`.
fn fill(
    song: &crate::song::Song,
    piece: &crate::hits::KitPiece,
    media: &Path,
    dry: bool,
    out: &mut FromMicsOut,
) -> Result<(), String> {
    let (daw, project) = (song.daw(), song.context());
    let Some(mic) = piece.mics.first() else {
        return Ok(());
    };
    let Some(mic_track) = Tracks::get(daw, project.clone(), TrackRef::Guid(mic.clone())) else {
        return Ok(());
    };
    if Tracks::get(daw, project.clone(), TrackRef::Guid(piece.trigger.clone())).is_none() {
        return Ok(());
    }
    let comp = if mic_track.lane_count > 0 {
        crate::comp::comp_lane_of(daw, &project, &mic_track)
    } else {
        None
    };
    let kept = |i: &daw_proto::Item| {
        !i.muted
            && (crate::drum_edit::is_edited(&mic_track, i)
                || (comp.is_some() && i.fixed_lane.unwrap_or(0) == comp.unwrap_or(0)))
    };
    let placed = |i: &daw_proto::Item| -> Placed {
        let at = i.position.as_seconds();
        (i.fixed_lane, at, at + i.length.as_seconds())
    };
    let mic_items: Vec<daw_proto::Item> = daw
        .get_items(project.clone(), TrackRef::Guid(mic.clone()))
        .into_iter()
        .filter(|i| kept(i))
        .collect();
    let trig: Vec<Placed> = daw
        .get_items(project.clone(), TrackRef::Guid(piece.trigger.clone()))
        .iter()
        .filter(|i| !i.muted)
        .map(placed)
        .collect();
    let mic_placed: Vec<Placed> = mic_items.iter().map(placed).collect();
    // The items with no trigger, by the recording they are cut from.
    let mut by_source: std::collections::BTreeMap<String, Vec<(daw_proto::Item, daw_proto::Take)>> =
        std::collections::BTreeMap::new();
    for i in uncovered(&mic_placed, &trig) {
        let Some(item) = mic_items.get(i) else {
            continue;
        };
        let Some(take) = daw.get_active_take(project.clone(), ItemRef::Guid(item.guid.clone()))
        else {
            continue;
        };
        let Some(source) = take.source_file_path.clone() else {
            continue;
        };
        by_source
            .entry(source)
            .or_default()
            .push((item.clone(), take));
    }
    let bay = daw.media_bay();
    for (source, items) in by_source {
        let Some((first, _)) = items.first() else {
            continue;
        };
        let markers = daw.get_take_markers(
            project.clone(),
            ItemRef::Guid(first.guid.clone()),
            TakeRef::Active,
        );
        let hits = sure_hits(&markers, &piece.name);
        if hits.is_empty() {
            out.left.push(format!(
                "{}: {source} (no trigger, and no sure hit on its mic to make one from)",
                piece.name
            ));
            continue;
        }
        let mut played: Vec<(f64, f64)> = items
            .iter()
            .filter(|(i, _)| crate::drum_edit::is_edited(&mic_track, i))
            .map(|(i, _)| {
                let at = i.position.as_seconds();
                (at, at + i.length.as_seconds())
            })
            .collect();
        played.sort_by(|a, b| a.0.total_cmp(&b.0));
        let stretches = merged(&played)
            .into_iter()
            .map(|(from, to)| {
                let (bar, beat, frac) = crate::song_bars::musical(daw, &project, from);
                format!(
                    "{from:.3}–{to:.3} s (bar {bar} beat {:.2}), {:.2} s",
                    f64::from(beat) + frac,
                    to - from
                )
            })
            .collect();
        let to = fresh(media, &made_name(&source)).display().to_string();
        out.filled.push(Filled {
            piece: piece.name.clone(),
            from: source.clone(),
            to: to.clone(),
            clicks: u32::try_from(hits.len()).unwrap_or(u32::MAX),
            items: u32::try_from(items.len()).unwrap_or(u32::MAX),
            stretches,
        });
        if dry {
            continue;
        }
        let disk = bay
            .resolve_file_path(&source)
            .ok_or_else(|| format!("{source} is not on this machine"))?;
        write_made(&disk, Path::new(&to), &hits)?;
        // The take's hits are where they were, in a file of the same
        // length: its markers go with it.
        let _ = daw.write_project(song.id().as_str(), |p| {
            if let Some(marks) = p.take_markers.get(&source).cloned() {
                p.take_markers.insert(to.clone(), marks);
            }
        });
        for (item, take) in &items {
            mirror(song, &piece.trigger, (item, take), &to)?;
        }
    }
    Ok(())
}

/// An item on `trigger` where `item` (a mic's) is — its lane, its slice
/// of the source, its rate, fades and stretch markers — playing `to`.
fn mirror(
    song: &crate::song::Song,
    trigger: &str,
    (item, take): (&daw_proto::Item, &daw_proto::Take),
    to: &str,
) -> Result<(), String> {
    let (daw, project) = (song.daw(), song.context());
    let made = daw
        .add_item(
            project.clone(),
            TrackRef::Guid(trigger.to_owned()),
            item.position,
            item.length,
        )
        .ok_or("the trigger's item could not be made")?;
    let at = ItemRef::Guid(made);
    let failed = |e: daw_proto::DawError| e.to_string();
    if let Some(lane) = item.fixed_lane {
        daw.set_fixed_lane(project.clone(), at.clone(), lane)
            .map_err(failed)?;
    }
    daw.set_source_file(project.clone(), at.clone(), TakeRef::Active, to.to_owned())
        .map_err(failed)?;
    daw.set_start_offset(
        project.clone(),
        at.clone(),
        TakeRef::Active,
        take.start_offset,
    )
    .map_err(failed)?;
    if take.play_rate > 0.0 && (take.play_rate - 1.0).abs() > f64::EPSILON {
        daw.set_play_rate(project.clone(), at.clone(), TakeRef::Active, take.play_rate)
            .map_err(failed)?;
    }
    daw.set_fade_in(
        project.clone(),
        at.clone(),
        item.fade_in_length,
        item.fade_in_shape,
    )
    .map_err(failed)?;
    daw.set_fade_out(
        project.clone(),
        at.clone(),
        item.fade_out_length,
        item.fade_out_shape,
    )
    .map_err(failed)?;
    let stretch = daw_proto::StretchMarkers::get_stretch_markers(
        daw,
        project.clone(),
        ItemRef::Guid(item.guid.clone()),
        TakeRef::Active,
    );
    if !stretch.is_empty() {
        daw_proto::StretchMarkers::set_stretch_markers(
            daw,
            daw_proto::StretchTakeRef::new(project.clone(), at.clone(), TakeRef::Active),
            stretch,
        )
        .map_err(failed)?;
    }
    let made_take = daw
        .get_active_take(project, at)
        .ok_or("the trigger's item has no take")?;
    daw::standalone::audio_engine::materialize::materialize_take_via_bay(
        daw,
        song.id().as_str(),
        &made_take.guid,
        to,
    )
}

/// `DRUMS_TRIGGERS_FROM_MICS`.
///
/// # Errors
/// When the action is already registered.
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&FROM_MICS, Effect::Mutating, run)?;
    Ok(())
}

#[cfg(test)]
mod tests;
