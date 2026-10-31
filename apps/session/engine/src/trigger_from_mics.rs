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

use std::path::{Path, PathBuf};

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{Items as _, Takes as _};
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

static FROM_MICS: ActionMeta = crate::actions::meta(
    "DRUMS_TRIGGERS_FROM_MICS",
    "drums_triggers_from_mics",
    "Triggers from the mics",
    "Where a take's trigger recorded nothing for its drum (none of the piece's hit markers on that recording were heard by the trigger, while its close mics heard some), make the trigger from the mics: a file in the song's own Media folder, `<name> (from mics).wav`, a 1 ms click at each hit the mics heard (confidence 0.5 or more, not bleed) — the hits the kit's MIDI was made from — and every take cut from the dead file plays it instead, its hit markers with it. The drum's gate is then keyed on the drum as played. Run after HITS_DETECT. `pieces` (all when left out); `dry_run` says what it would do. One undo step. Forks a checkpoint of the song first (`Before DRUMS_TRIGGERS_FROM_MICS <date time>`, named in the result's `checkpoint`; open it as an alternative to go back), and is refused if one cannot be made.",
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

/// What it did, or would.
#[derive(Facet)]
struct FromMicsOut {
    made: Vec<Made>,
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

/// Write the trigger for `hits` into `media` (the song's own folder) for
/// `disk` (the dead recording): its rate, its length, one channel. Where
/// it went.
fn write_made(disk: &Path, media: &Path, hits: &[f64]) -> Result<PathBuf, String> {
    let pcm = fts_sample::mapped::PcmFile::open(disk).map_err(|e| e.to_string())?;
    let rate = pcm.sample_rate();
    let frames = pcm.frames();
    let file = disk
        .file_name()
        .and_then(|f| f.to_str())
        .ok_or("the recording has no name")?;
    std::fs::create_dir_all(media).map_err(|e| e.to_string())?;
    let path = media.join(made_name(file));
    fts_sample::write_wav_f32(&path, rate, &[clicks(hits, frames, rate)])
        .map_err(|e| e.to_string())?;
    Ok(path)
}

/// `DRUMS_TRIGGERS_FROM_MICS`: what [`make`] would do, then — when it
/// would make anything and this is not a dry run — a checkpoint, and it
/// done.
fn run(a: FromMicsArg) -> Result<FromMicsOut, String> {
    let plan = make(&FromMicsArg {
        pieces: a.pieces.clone(),
        dry_run: true,
    })?;
    if a.dry_run || plan.made.is_empty() {
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
                let to = media.join(made_name(&rec.source)).display().to_string();
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
                write_made(&disk, &media, &hits)?;
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
        Ok(())
    })();
    if !a.dry_run {
        daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
        crate::song_model::request_resync();
    }
    done?;
    Ok(out)
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
