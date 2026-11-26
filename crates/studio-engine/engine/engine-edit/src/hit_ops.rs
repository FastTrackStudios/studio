//! The drum edits an agent could not make before: a stroke's sound
//! deleted (`HITS_DELETE`), the owner's corrections by bar and beat
//! (`DRUMS_FIX`), and a take switch trimmed so two takes never sound
//! together (`COMP_SEAM_TRIM`). The first two go through `DRUMS_APPLY`.

use std::collections::HashMap;

use architect::action::{ActionList, ActionMeta, Effect};
use daw::standalone::item_ops::ItemOp;
use daw_proto::id::ItemId;
use facet::Facet;

use crate::actions::meta;
use crate::drum_apply::{Applied, ApplyArg, Stroke};
use crate::edit_list::{parse_at, time_of};

/// How far from an address the hit it names may play, seconds: a hit
/// played early or late is still the one the owner heard there.
pub const FIX_REACH: f64 = 0.060;

static DELETE: ActionMeta = meta(
    "HITS_DELETE",
    "hits_delete",
    "Delete drum strokes",
    "Remove the strokes `ids` from the sound, deterministically: on every mic their slice is cut out and the slice before drawn out to meet the next (never slower than 0.8×; what follows is moved up when it would be), their markers gone. A stroke struck with another drum's (within 15 ms) cannot be cut out alone: delete both, or remove its marker (HITS_REMOVE). DRUMS_APPLY `delete`, with its check. `mark` names a restore point; `dry_run` says what it would do. One undo step.",
    "Drums",
    "",
);

static FIX: ActionMeta = meta(
    "DRUMS_FIX",
    "drums_fix",
    "Fix drum hits by bar and beat",
    "Correct the drum edit the way the owner says it: `at` a bar.beat address (`118.4.25`: bar 118, beat 4 and a quarter) with `to` (`118.4.5`; or `to_s` seconds, or `by_ms`) moves the hit playing nearest there (within 60 ms) and what is struck with it; `remove: \"150.1\"` takes the marker of the hit there away (a false detection: its sound rides the slice before); `delete: …` takes its sound away. `piece` names the drum when two share the beat (the others struck with it then ride it). Several at once: `fixes: [{at, to}, {remove}, …]`. Resolved to hit ids (said in `resolved`) and applied as one DRUMS_APPLY, its result in `apply`. `mark`, `dry_run` as DRUMS_APPLY.",
    "Drums",
    "",
);

static SEAM_TRIM: ActionMeta = meta(
    "COMP_SEAM_TRIM",
    "comp_seam_trim",
    "Trim a take switch",
    "At the take switch nearest `at` (project seconds, within 1 s): the old take ends `before_ms` (3 unless given) ahead of the first attack there — the new take's, or the old take's own where it comes first — on every mic's EDIT lane, the new take reaching back to crossfade in before that; so two takes never sound together on a downbeat. Nothing else of the comp or the edit moves (COMP_CHOOSE over a short stretch moves the switch to a take's cut point, maybe a beat away). DRUMS_EDIT_LANES `reset` copies the COMP lane again, and with it the untrimmed switch. `dry_run` says what it would cut. One undo step.",
    "Drums",
    "",
);

#[derive(Facet)]
struct DeleteArg {
    ids: Vec<String>,
    mark: Option<String>,
    dry_run: Option<bool>,
}

/// One correction.
#[derive(Facet, Default, Clone)]
struct Fix {
    at: Option<String>,
    to: Option<String>,
    to_s: Option<f64>,
    by_ms: Option<f64>,
    remove: Option<String>,
    delete: Option<String>,
    piece: Option<String>,
}

#[derive(Facet)]
struct FixArg {
    at: Option<String>,
    to: Option<String>,
    to_s: Option<f64>,
    by_ms: Option<f64>,
    remove: Option<String>,
    delete: Option<String>,
    piece: Option<String>,
    #[facet(default)]
    fixes: Vec<Fix>,
    mark: Option<String>,
    dry_run: Option<bool>,
}

/// What an address resolved to.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct Resolved {
    /// The address as given.
    pub address: String,
    /// `move`, `remove` or `delete`.
    pub what: String,
    pub ids: Vec<String>,
    /// Where the nearest of them plays now, and where it is sent, s.
    pub plays: f64,
    pub to: Option<f64>,
}

#[derive(Facet)]
struct Fixed {
    resolved: Vec<Resolved>,
    apply: Applied,
}

/// The hits an address names: the stroke (of `piece`, when given) playing
/// nearest `t` within [`FIX_REACH`], and those struck with it.
/// `(the nearest, every one)`, as stroke indices.
///
/// # Errors
/// No hit plays near there.
pub fn named_at(
    strokes: &[Stroke],
    t: f64,
    piece: Option<&str>,
) -> Result<(usize, Vec<usize>), String> {
    let near = strokes
        .iter()
        .enumerate()
        .filter(|(_, s)| piece.is_none_or(|p| s.piece.eq_ignore_ascii_case(p)))
        .map(|(k, s)| (k, (s.now - t).abs()))
        .filter(|(_, d)| *d <= FIX_REACH)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(k, _)| k)
        .ok_or_else(|| {
            format!(
                "no {}hit plays within {:.0} ms of {t:.3} s",
                piece.map(|p| format!("{p} ")).unwrap_or_default(),
                FIX_REACH * 1_000.0
            )
        })?;
    let at = strokes.get(near).map_or(t, |s| s.now);
    let with: Vec<usize> = strokes
        .iter()
        .enumerate()
        .filter(|(_, s)| (s.now - at).abs() <= editing::drums::KIT_MERGE)
        .map(|(k, _)| k)
        .collect();
    Ok((near, with))
}

/// Resolve `fixes` (each its address in seconds already, `when`) to an
/// apply: targets, removes, deletes and rides.
///
/// # Errors
/// An address with no hit near it, or a fix that says nothing to do.
pub fn resolve(
    strokes: &[Stroke],
    fixes: &[(String, Option<&str>, FixKind)],
) -> Result<(ApplyArg, Vec<Resolved>), String> {
    let mut arg = ApplyArg::default();
    let mut out = Vec::new();
    for (address, piece, kind) in fixes {
        let at = match kind {
            FixKind::Move { at, .. } | FixKind::Remove(at) | FixKind::Delete(at) => *at,
        };
        let (near, with) = named_at(strokes, at, *piece)?;
        let Some(nearest) = strokes.get(near) else {
            continue;
        };
        // With a drum named, only its hit; else everything struck there.
        let chosen: Vec<usize> = if piece.is_some() {
            vec![near]
        } else {
            with.clone()
        };
        let ids: Vec<String> = chosen
            .iter()
            .filter_map(|k| strokes.get(*k).map(|s| s.id.clone()))
            .collect();
        let (what, to) = match kind {
            FixKind::Move { to, .. } => {
                let to = match to {
                    To::At(t) => *t,
                    To::By(ms) => nearest.now + ms / 1_000.0,
                };
                let by = to - nearest.now;
                for k in &chosen {
                    if let Some(s) = strokes.get(*k) {
                        arg.targets.insert(s.id.clone(), s.now + by);
                    }
                }
                // The others struck with it ride it.
                for k in with.iter().filter(|k| !chosen.contains(k)) {
                    if let Some(s) = strokes.get(*k) {
                        arg.rides.insert(s.id.clone(), nearest.id.clone());
                    }
                }
                ("move", Some(to))
            }
            FixKind::Remove(_) => {
                arg.remove.extend(ids.iter().cloned());
                ("remove", None)
            }
            FixKind::Delete(_) => {
                arg.delete.extend(ids.iter().cloned());
                ("delete", None)
            }
        };
        out.push(Resolved {
            address: address.clone(),
            what: what.to_owned(),
            ids,
            plays: nearest.now,
            to,
        });
    }
    Ok((arg, out))
}

/// What a fix does, its address in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FixKind {
    Move { at: f64, to: To },
    Remove(f64),
    Delete(f64),
}

/// Where a moved hit goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum To {
    /// Project seconds.
    At(f64),
    /// This many ms from where it plays.
    By(f64),
}

/// A fix's address and target in seconds.
fn kind_of(
    daw: &daw::standalone::Standalone,
    project: &daw::service::ProjectContext,
    fix: &Fix,
) -> Result<(String, FixKind), String> {
    let when = |text: &str| parse_at(text).map(|at| time_of(daw, project, at));
    if let Some(at) = &fix.at {
        let t = when(at)?;
        let (to, said) = match (&fix.to, fix.to_s, fix.by_ms) {
            (Some(to), _, _) => (To::At(when(to)?), to.clone()),
            (None, Some(s), _) => (To::At(s), format!("{s:.4} s")),
            (None, None, Some(ms)) => (To::By(ms), format!("{ms:+} ms")),
            (None, None, None) => {
                return Err(format!("{at}: say where it goes (`to`, `to_s` or `by_ms`)"));
            }
        };
        return Ok((format!("{at} → {said}"), FixKind::Move { at: t, to }));
    }
    if let Some(at) = &fix.remove {
        return Ok((at.clone(), FixKind::Remove(when(at)?)));
    }
    if let Some(at) = &fix.delete {
        return Ok((at.clone(), FixKind::Delete(when(at)?)));
    }
    Err("a fix says `at` (with `to`), `remove` or `delete`".to_owned())
}

fn fix(a: FixArg) -> Result<Fixed, String> {
    let span = tracing::info_span!(
        "DRUMS_FIX",
        fixes = tracing::field::Empty,
        ids = tracing::field::Empty
    );
    let _in = span.enter();
    let (daw, project) = crate::actions::song()?;
    let top = Fix {
        at: a.at,
        to: a.to,
        to_s: a.to_s,
        by_ms: a.by_ms,
        remove: a.remove,
        delete: a.delete,
        piece: a.piece,
    };
    let all: Vec<Fix> = std::iter::once(top)
        .filter(|f| f.at.is_some() || f.remove.is_some() || f.delete.is_some())
        .chain(a.fixes)
        .collect();
    if all.is_empty() {
        return Err("nothing to fix: say `at` and `to`, `remove`, `delete` or `fixes`".to_owned());
    }
    let step = tracing::info_span!("DRUMS_FIX.resolve").entered();
    let strokes = crate::drum_apply::strokes(daw, &project);
    let mut kinds = Vec::new();
    for f in &all {
        let (address, kind) = kind_of(daw, &project, f)?;
        kinds.push((address, f.piece.as_deref(), kind));
    }
    let (mut arg, resolved) = resolve(&strokes, &kinds)?;
    drop(step);
    span.record("fixes", resolved.len());
    span.record("ids", resolved.iter().map(|r| r.ids.len()).sum::<usize>());
    arg.mark = a.mark;
    arg.dry_run = a.dry_run;
    let apply = crate::drum_apply::apply(arg)?;
    Ok(Fixed { resolved, apply })
}

// ── the seam trim ─────────────────────────────────────────────────────

/// How far from `at` a take switch is looked for, seconds.
const SEAM_REACH: f64 = 1.0;

#[derive(Facet)]
struct SeamArg {
    at: f64,
    before_ms: Option<f64>,
    dry_run: Option<bool>,
}

/// What a seam trim did.
#[derive(Facet, Debug, Default)]
struct Trimmed {
    /// Where the new take came in, as the reference mic has it, s.
    seam: f64,
    /// The first attack there, and where the old take now ends.
    attack: f64,
    cut: f64,
    /// Mics trimmed, and those that could not be (a take's start drawn
    /// with stretch markers).
    mics: u32,
    skipped: Vec<String>,
    /// How much of the old take was taken off, ms (the most on any mic).
    trimmed_ms: f64,
    applied: bool,
}

/// One mic's old and new slice at a switch near `seam`.
fn switch_on(lane: &[crate::hit_place::Slice], seam: f64) -> Option<(usize, usize)> {
    lane.windows(2)
        .enumerate()
        .filter(|(_, w)| w[0].file != w[1].file && (w[1].start() - seam).abs() <= 0.030)
        .min_by(|a, b| {
            (a.1[1].start() - seam)
                .abs()
                .total_cmp(&(b.1[1].start() - seam).abs())
        })
        .map(|(i, _)| (i, i + 1))
}

/// The ops that end `old` at `cut` and bring `new` in to crossfade before
/// it; `None` when the new slice cannot reach back (drawn with markers).
fn trim_ops(
    old: &crate::hit_place::Slice,
    new: &crate::hit_place::Slice,
    cut: f64,
    xfade: f64,
) -> Option<Vec<ItemOp>> {
    let mut ops = Vec::new();
    let old_id = ItemId::from(old.item.guid.as_str());
    let length = (cut - old.start()).max(0.001);
    ops.push(ItemOp::Place {
        item: old_id.clone(),
        at: old.start(),
        length,
        offset: old.item.start_offset.as_seconds(),
    });
    ops.push(ItemOp::Fades {
        item: old_id,
        fade_in: old.item.fade_in_length.as_seconds().min(length / 2.0),
        fade_out: xfade.min(length / 2.0),
    });
    let new_id = ItemId::from(new.item.guid.as_str());
    let start = (cut - xfade).min(new.start());
    let reach = new.start() - start;
    if reach > 1e-6 {
        let offset = new.item.start_offset.as_seconds() - reach;
        if offset < 0.0 || new.markers.len() > 1 {
            return None;
        }
        ops.push(ItemOp::Place {
            item: new_id.clone(),
            at: start,
            length: new.end() - start,
            offset,
        });
    }
    ops.push(ItemOp::Fades {
        item: new_id,
        // In over the stretch both play: from its start to the cut.
        fade_in: (cut - start).max(0.0),
        fade_out: new.item.fade_out_length.as_seconds(),
    });
    Some(ops)
}

fn seam_trim(a: SeamArg) -> Result<Trimmed, String> {
    let span = tracing::info_span!(
        "COMP_SEAM_TRIM",
        mics = tracing::field::Empty,
        trimmed_ms = tracing::field::Empty
    );
    let _in = span.enter();
    let (daw, project) = crate::actions::song()?;
    let song = crate::song::Song::current()?;
    let settings = crate::drum_settings::current();
    let (lead, xfade) = (settings.lead_ms / 1_000.0, settings.xfade_ms / 1_000.0);
    let before = a.before_ms.unwrap_or(3.0) / 1_000.0;
    let step = tracing::info_span!("COMP_SEAM_TRIM.read").entered();
    let mics = crate::hit_place::mics(&song);
    let window = (a.at - SEAM_REACH - 5.0, a.at + SEAM_REACH + 5.0);
    let lanes: Vec<(String, Vec<crate::hit_place::Slice>)> = song
        .read(|p| {
            mics.iter()
                .map(|m| {
                    (
                        m.track.clone(),
                        crate::hit_place::lane_of(p, &m.track, m.edit, window),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    // The switch: on the first mic that has one, the boundary nearest `at`.
    let seam = lanes
        .iter()
        .flat_map(|(_, lane)| {
            lane.windows(2)
                .filter(|w| w[0].file != w[1].file)
                .map(|w| w[1].start())
                .collect::<Vec<_>>()
        })
        .filter(|s| (s - a.at).abs() <= SEAM_REACH)
        .min_by(|x, y| (x - a.at).abs().total_cmp(&(y - a.at).abs()))
        .ok_or_else(|| format!("no take switch within {SEAM_REACH} s of {:.3} s", a.at))?;
    let attack = crate::hits::playback(daw, &project)
        .iter()
        .map(|p| p.time)
        .filter(|t| *t >= seam - 0.030 && *t <= seam + 0.060)
        .fold(f64::INFINITY, f64::min);
    let attack = if attack.is_finite() {
        attack
    } else {
        seam + lead
    };
    let cut = attack - before;
    drop(step);
    let step = tracing::info_span!("COMP_SEAM_TRIM.cut").entered();
    let mut out = Trimmed {
        seam,
        attack,
        cut,
        ..Trimmed::default()
    };
    let names: HashMap<String, String> = daw::service::Tracks::all(daw, project.clone())
        .into_iter()
        .map(|t| (t.guid, t.name))
        .collect();
    let mut ops = Vec::new();
    for (track, lane) in &lanes {
        let Some((o, n)) = switch_on(lane, seam) else {
            continue;
        };
        let (Some(old), Some(new)) = (lane.get(o), lane.get(n)) else {
            continue;
        };
        if old.end() <= cut + 1e-6 {
            continue;
        }
        match trim_ops(old, new, cut, xfade) {
            Some(mut more) => {
                out.mics = out.mics.saturating_add(1);
                out.trimmed_ms = out
                    .trimmed_ms
                    .max(((old.end() - cut) * 10_000.0).round() / 10.0);
                ops.append(&mut more);
            }
            None => out
                .skipped
                .push(names.get(track).cloned().unwrap_or_else(|| track.clone())),
        }
    }
    drop(step);
    span.record("mics", out.mics);
    span.record("trimmed_ms", out.trimmed_ms);
    if a.dry_run.unwrap_or(false) || ops.is_empty() {
        return Ok(out);
    }
    let _s = tracing::info_span!("COMP_SEAM_TRIM.write", ops = ops.len()).entered();
    song.undo_step("Trim a take switch", |song| song.apply(&ops))
        .map_err(|e| e.to_string())?;
    crate::song_model::request_resync_items();
    crate::hit_overlay::refresh();
    crate::actions::list().changed("items", "seam trimmed");
    out.applied = true;
    Ok(out)
}

/// Register `HITS_DELETE`, `DRUMS_FIX` and `COMP_SEAM_TRIM`.
///
/// # Errors
/// When one is already registered.
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&DELETE, Effect::Mutating, |a: DeleteArg| {
        let _in = tracing::info_span!("HITS_DELETE", ids = a.ids.len()).entered();
        crate::drum_apply::apply(ApplyArg {
            delete: a.ids,
            mark: a.mark,
            dry_run: a.dry_run,
            ..ApplyArg::default()
        })
    })?;
    list.register(&FIX, Effect::Mutating, fix)?;
    list.register(&SEAM_TRIM, Effect::Mutating, seam_trim)?;
    Ok(())
}
