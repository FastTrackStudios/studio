//! The kit comped from its best takes — over a stretch or the whole song —
//! as one edit.
//!
//! Every take's hits, the part as written and what the comp plays now are
//! read once; each phrase is ranked from them; the phrases whose best take
//! is not the one playing are comped together: one list of item edits, one
//! undo step, one look by the window at what changed. A hand does it with
//! Ctrl+T, a model with `COMP_BEST`.

use std::collections::{BTreeMap, HashMap};

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{TrackRef, Tracks as _};
use daw::standalone::item_ops::ItemOp;
use daw_proto::id::ItemId;
use editing::comp::Strike;
use facet::Facet;

use crate::hits::TakeVersion;
use crate::song::Song;

/// How many bars a phrase spans unless a call says.
const PHRASE_BARS: f64 = 4.0;

/// A phrase and the take it is comped from.
#[derive(Clone, Debug)]
pub struct Choice {
    pub from: f64,
    pub to: f64,
    /// The take's recording pass.
    pub take: String,
    pub score: f64,
}

/// What the comp found and did.
#[derive(Debug, Default)]
pub struct Comped {
    /// The phrases ranked.
    pub phrases: usize,
    /// Those comped from another take.
    pub choices: Vec<Choice>,
    /// The item edits it took.
    pub ops: usize,
}

/// The phrases of `span`: each section cut into stretches of about
/// `bars` bars, clipped to `span`.
#[must_use]
pub fn phrases(song: &Song, (from, to): (f64, f64), bars: f64) -> Vec<(f64, f64)> {
    let bars = bars.max(1.0);
    crate::actions::sections(song.daw(), &song.context())
        .into_iter()
        .filter(|s| s.end > from && s.start < to)
        .flat_map(|s| {
            let count = f64::from((s.last_bar - s.first_bar + 1).max(1));
            let parts = (count / bars).ceil().max(1.0);
            let step = (s.end - s.start) / parts;
            let n = num_traits::ToPrimitive::to_usize(&parts).unwrap_or(1);
            (0..n).map(move |k| {
                let k = num_traits::ToPrimitive::to_f64(&k).unwrap_or(0.0);
                (s.start + k * step, s.start + (k + 1.0) * step)
            })
        })
        .map(|(a, b)| (a.max(from), b.min(to)))
        .filter(|(a, b)| b - a > 0.05)
        .collect()
}

/// Comp `phrases` from their best takes, as one edit: each phrase ranked
/// from one read of the takes, the part as written and the comp; those
/// whose best judged take is not the one playing comped together. With
/// `dry`, only says what it would comp.
///
/// # Errors
/// When the engine refuses the edit.
pub fn comp_best(song: &Song, phrases: &[(f64, f64)], dry: bool) -> Result<Comped, String> {
    let mut stages = crate::stages::Stages::start();
    let (Some(first), Some(last)) = (phrases.first(), phrases.last()) else {
        return Ok(Comped::default());
    };
    let span = (first.0, last.1);
    let (daw, project) = (song.daw(), song.context());
    let versions = crate::comp_rank::versions_over(daw, &project, span);
    let written = crate::comp_rank::written(daw, &project, span, &versions);
    let playing = Playing::read(song, &versions, span);
    let audio = Audio::read(song, &versions, span);
    stages.mark("comp.read_ms");
    // Each seam between phrases just before the hits there, in every take:
    // a seam on the beat cut between two takes that do not hit it at the
    // same instant, and both attacks played — a flam.
    let attacks: Vec<f64> = {
        let mut all: Vec<f64> = versions
            .iter()
            .flat_map(|v| v.hits.iter().map(|h| h.1))
            .collect();
        all.sort_by(f64::total_cmp);
        all
    };
    let phrases: Vec<(f64, f64)> = clear_of_hits(phrases, &attacks);

    let line_of = |t: f64| crate::actions::grid_line(daw, &project, t, 16);
    let main = crate::comp_keep::main_take(daw, &project);
    let choices: Vec<Choice> = phrases
        .iter()
        .filter_map(|&(from, to)| {
            // Only a take with audio on its lanes over the whole phrase can
            // be comped from there: its hits may be marked on a recording
            // its lane plays only part of.
            // Nor one ranked down by hand there.
            let mid = f64::midpoint(from, to);
            let usable: Vec<TakeVersion> = versions
                .iter()
                .filter(|v| audio.covers(v, (from, to)))
                .filter(|v| {
                    v.files.first().is_none_or(|(track, path)| {
                        crate::take_rank::level_of_take(daw, &project, (track, path), mid) >= 0
                    })
                })
                .cloned()
                .collect();
            let here = within(&usable, (from, to));
            let notes = strikes_within(&written, (from, to));
            // With a main take that plays the phrase: held up to it, and
            // left only for a take alike to it that plays clearly better.
            let main_here = main
                .as_deref()
                .filter(|m| here.iter().any(|v| v.pass == *m && !v.hits.is_empty()));
            let ranked =
                crate::comp_rank::rank_versions(&here, &notes, (None, main_here), &line_of);
            let best = match main_here {
                Some(m) => stay_or_leave(&ranked, m)?,
                None => ranked.iter().find(|t| !t.undetected && !t.empty)?,
            };
            // Already playing: its own recordings are what the comp plays
            // there (a recording two passes share says nothing of which).
            let version = versions.iter().find(|v| v.pass == best.take)?;
            if playing.plays(version, f64::midpoint(from, to)) {
                return None;
            }
            Some(Choice {
                from,
                to,
                take: best.take.clone(),
                score: best.score,
            })
        })
        .collect();
    stages.mark("comp.rank_ms");
    if dry || choices.is_empty() {
        return Ok(Comped {
            phrases: phrases.len(),
            choices,
            ops: 0,
        });
    }

    let edit = Edit::plan(song, &versions, &choices);
    stages.mark("comp.plan_ms");
    let ops = edit.ops.len();
    song.undo_step("Comp the best takes", |song| edit.apply(song))?;
    stages.mark("comp.apply_ms");
    crate::comp_rank::forget();
    crate::studio::request_resync_items();
    crate::hit_overlay::refresh();
    Ok(Comped {
        phrases: phrases.len(),
        choices,
        ops,
    })
}

/// How much better (in score) a take must play a phrase than the main
/// take to be comped there instead.
const CLEARLY_BETTER: f64 = 0.25;
/// How alike to the main take's version a take must be to stand in for
/// it: the same part, played better — not another part.
const STANDS_IN: f64 = 0.8;

/// Of `ranked` (held up to the main take `main`), the take to comp: the
/// main take, unless a take alike to it ([`STANDS_IN`]) scores
/// [`CLEARLY_BETTER`]. `None` when nothing plays.
fn stay_or_leave<'a>(
    ranked: &'a [crate::comp_rank::TakeRank],
    main: &str,
) -> Option<&'a crate::comp_rank::TakeRank> {
    let judged = |t: &&crate::comp_rank::TakeRank| !t.undetected && !t.empty;
    let Some(stay) = ranked.iter().filter(judged).find(|t| t.pass == main) else {
        return ranked.iter().find(judged);
    };
    let leave = ranked
        .iter()
        .filter(judged)
        .filter(|t| t.pass != main && t.similar >= STANDS_IN)
        .max_by(|a, b| a.score.total_cmp(&b.score));
    Some(match leave {
        Some(t) if t.score >= stay.score + CLEARLY_BETTER => t,
        _ => stay,
    })
}

/// How far either side of a phrase's edge its seam looks for the hits it
/// must not cut through, seconds: more than any two takes of one beat are
/// apart.
pub(crate) const SEAM_REACH: f64 = 0.06;

/// How far before the earliest of those hits a seam is put, seconds: the
/// stick's approach is in the recording just before the transient.
pub(crate) const SEAM_LEAD: f64 = 0.010;

/// `phrases` (in order, meeting end to start) with each seam between two
/// moved to just before the earliest of `attacks` (sorted, every take's)
/// within [`SEAM_REACH`] of it.
fn clear_of_hits(phrases: &[(f64, f64)], attacks: &[f64]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = phrases.to_vec();
    for k in 1..out.len() {
        let seam = out[k].0;
        if (out[k - 1].1 - seam).abs() > 1e-6 {
            continue;
        }
        let moved = clear_seam(seam, attacks).max(out[k - 1].0 + 1e-3);
        out[k - 1].1 = moved;
        out[k].0 = moved;
    }
    out
}

/// A seam at `seam` moved to just before the earliest of `attacks`
/// (sorted, every take's) within [`SEAM_REACH`] of it; where none is, left.
/// Cut on the beat between two takes that do not hit it at the same
/// instant, both attacks would play — a flam — or the edit would find two
/// hits for one line and push one a sixteenth on.
#[must_use]
pub(crate) fn clear_seam(seam: f64, attacks: &[f64]) -> f64 {
    let first = attacks.partition_point(|t| *t < seam - SEAM_REACH);
    attacks
        .get(first)
        .filter(|t| **t <= seam + SEAM_REACH)
        .map_or(seam, |earliest| earliest - SEAM_LEAD)
}

/// `versions` with only their hits over `from..to`.
fn within(versions: &[TakeVersion], (from, to): (f64, f64)) -> Vec<TakeVersion> {
    versions
        .iter()
        .map(|v| {
            let lo = v.hits.partition_point(|h| h.1 < from);
            let hi = v.hits.partition_point(|h| h.1 < to);
            TakeVersion {
                hits: v.hits.get(lo..hi).map(<[_]>::to_vec).unwrap_or_default(),
                ..v.clone()
            }
        })
        .collect()
}

fn strikes_within(written: &[Strike], (from, to): (f64, f64)) -> Vec<Strike> {
    written
        .iter()
        .filter(|s| s.t >= from && s.t < to)
        .cloned()
        .collect()
}

/// Where each take's lanes hold audio, by `(track, lane)`: spans merged,
/// by start.
pub(crate) struct Audio {
    by_lane: HashMap<(String, u32), Vec<(f64, f64)>>,
}

impl Audio {
    pub(crate) fn read(song: &Song, versions: &[TakeVersion], (from, to): (f64, f64)) -> Self {
        let wanted: std::collections::BTreeSet<(String, u32)> = versions
            .iter()
            .flat_map(|v| v.lanes.iter().cloned())
            .collect();
        let by_lane = song
            .read(|p| {
                wanted
                    .iter()
                    .map(|(track, lane)| {
                        let spans = crate::hit_place::lane_of(p, track, *lane, (from, to))
                            .iter()
                            .map(|s| (s.start(), s.end()))
                            .collect::<Vec<_>>();
                        ((track.clone(), *lane), union(spans.into_iter()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { by_lane }
    }

    /// Whether every lane of `version` holds audio over all of `from..to`.
    pub(crate) fn covers(&self, version: &TakeVersion, (from, to): (f64, f64)) -> bool {
        const SLACK: f64 = 0.002;
        !version.lanes.is_empty()
            && version.lanes.iter().all(|key| {
                self.by_lane.get(key).is_some_and(|spans| {
                    spans
                        .iter()
                        .any(|(a, b)| *a <= from + SLACK && *b >= to - SLACK)
                })
            })
    }
}

/// What each mic's COMP lane plays over the stretch: its items, each with
/// its recording.
struct Playing {
    /// By track: `(start, end, file)`, by start.
    by_track: HashMap<String, Vec<(f64, f64, String)>>,
}

impl Playing {
    fn read(song: &Song, versions: &[TakeVersion], (from, to): (f64, f64)) -> Self {
        let tracks: Vec<daw_proto::Track> = song.daw().all(song.context());
        let wanted: Vec<(String, u32)> = versions
            .iter()
            .flat_map(|v| v.files.iter().map(|(t, _)| t.clone()))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter_map(|guid| {
                let t = tracks.iter().find(|t| t.guid == guid)?;
                let comp = crate::drum_lanes::lane_named(t, crate::drum_lanes::COMP).unwrap_or(0);
                Some((guid, comp))
            })
            .collect();
        let by_track = song
            .read(|p| {
                wanted
                    .iter()
                    .map(|(track, comp)| {
                        let items = crate::hit_place::lane_of(p, track, *comp, (from, to))
                            .into_iter()
                            .filter_map(|s| Some((s.start(), s.end(), s.file?)))
                            .collect();
                        (track.clone(), items)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { by_track }
    }

    /// Whether the comp plays `version` at `t`: a COMP lane item there
    /// plays one of its recordings.
    fn plays(&self, version: &TakeVersion, t: f64) -> bool {
        version.files.iter().any(|(track, path)| {
            self.by_track.get(track).is_some_and(|items| {
                items
                    .iter()
                    .any(|(a, b, file)| *a <= t && t < *b && file == path)
            })
        })
    }
}

/// The comp of a set of phrases as item edits, and the comp areas it
/// leaves each track.
struct Edit {
    ops: Vec<ItemOp>,
    areas: Vec<(String, Vec<daw_proto::CompArea>)>,
}

impl Edit {
    /// Every mic's COMP lane over the phrases made its chosen take's lane,
    /// and its EDIT lane there the new comp as recorded.
    fn plan(song: &Song, versions: &[TakeVersion], choices: &[Choice]) -> Self {
        use crate::drum_lanes::{EDIT, lane_named};
        let tracks: Vec<daw_proto::Track> = song.daw().all(song.context());
        // Each mic and the phrases it is comped over, with the lane each is
        // comped from — none for a mic the take never recorded: silent
        // there, not another take's recording under this one's.
        let mut per_track: BTreeMap<String, Vec<(&Choice, Option<u32>)>> = BTreeMap::new();
        for choice in choices {
            let Some(version) = versions.iter().find(|v| v.pass == choice.take) else {
                continue;
            };
            let sources = version.lanes.iter().map(|(t, l)| (t.clone(), Some(*l)));
            let silent = version.unrecorded(versions).into_iter().map(|t| (t, None));
            for (track, source) in sources.chain(silent) {
                per_track.entry(track).or_default().push((choice, source));
            }
        }
        let mut ops = Vec::new();
        let mut areas = Vec::new();
        for (guid, phrases) in &per_track {
            let Some(track) = tracks.iter().find(|t| t.guid == *guid) else {
                continue;
            };
            let Some(comp) = crate::comp::comp_lane_of(song.daw(), &song.context(), track) else {
                continue;
            };
            let edit = lane_named(track, EDIT);
            let spans = union(phrases.iter().map(|(c, _)| (c.from, c.to)));
            let reach = (spans[0].0, spans[spans.len() - 1].1);
            let lanes = song.read(|p| {
                let lane = |n: u32| crate::hit_place::lane_of(p, guid, n, reach);
                let sources: BTreeMap<u32, Vec<daw_proto::Item>> = phrases
                    .iter()
                    .filter_map(|(_, source)| *source)
                    .map(|source| (source, lane(source).into_iter().map(|s| s.item).collect()))
                    .collect();
                (
                    lane(comp).into_iter().map(|s| s.item).collect::<Vec<_>>(),
                    edit.map(|e| lane(e).into_iter().map(|s| s.item).collect::<Vec<_>>()),
                    sources,
                )
            });
            let Some((comp_items, edit_items, sources)) = lanes else {
                continue;
            };
            for item in &comp_items {
                subtract_ops(item, &spans, &mut ops);
            }
            for item in edit_items.iter().flatten() {
                subtract_ops(item, &spans, &mut ops);
            }
            for (choice, source) in phrases {
                let Some(source) = source else {
                    continue;
                };
                for item in sources.get(source).into_iter().flatten() {
                    for lane in std::iter::once(comp).chain(edit) {
                        copy_ops(item, (choice.from, choice.to), lane, &mut ops);
                    }
                }
            }
            let mut kept = song
                .daw()
                .comping(song.context(), TrackRef::Guid(guid.clone()))
                .map(|c| c.areas)
                .unwrap_or_default();
            for (choice, source) in phrases.iter().filter_map(|(c, s)| Some((c, (*s)?))) {
                kept = crate::comp::with_area(
                    &kept,
                    daw_proto::CompArea {
                        start: daw_proto::PositionInSeconds::from_seconds(choice.from),
                        end: daw_proto::PositionInSeconds::from_seconds(choice.to),
                        source_lane: source,
                        comp_lane: comp,
                        fade_in: daw_proto::Duration::from_seconds(crate::comp::JOIN),
                        fade_out: daw_proto::Duration::from_seconds(crate::comp::JOIN),
                    },
                );
            }
            areas.push((guid.clone(), kept));
        }
        Self { ops, areas }
    }

    fn apply(&self, song: &Song) -> Result<(), String> {
        song.apply(&self.ops).map_err(|e| e.to_string())?;
        for (track, areas) in &self.areas {
            song.daw()
                .set_comp_areas(song.context(), TrackRef::Guid(track.clone()), areas.clone())
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// `spans` merged where they touch or overlap, by start.
fn union(spans: impl Iterator<Item = (f64, f64)>) -> Vec<(f64, f64)> {
    let mut all: Vec<(f64, f64)> = spans.collect();
    all.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (a, b) in all {
        match out.last_mut() {
            Some(last) if a <= last.1 + 1e-9 => last.1 = last.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// What takes `spans` (merged, by start) out of `item`: what is left of it
/// either side and between, the first part kept as the item itself, the
/// rest as copies; gone if nothing is left. A cut end has no fade; an end
/// of its own keeps its fade.
fn subtract_ops(item: &daw_proto::Item, spans: &[(f64, f64)], ops: &mut Vec<ItemOp>) {
    const GRAIN: f64 = 1e-6;
    let start = item.position.as_seconds();
    let end = start + item.length.as_seconds();
    if !spans
        .iter()
        .any(|(a, b)| *b > start + GRAIN && *a < end - GRAIN)
    {
        return;
    }
    let mut left: Vec<(f64, f64)> = Vec::new();
    let mut at = start;
    for &(a, b) in spans {
        if b <= at + GRAIN || a >= end - GRAIN {
            continue;
        }
        if a > at + GRAIN {
            left.push((at, a.min(end)));
        }
        at = at.max(b);
    }
    if at < end - GRAIN {
        left.push((at, end));
    }
    let id = ItemId::from(item.guid.as_str());
    if left.is_empty() {
        ops.push(ItemOp::Delete { item: id });
        return;
    }
    let offset = item.start_offset.as_seconds();
    for (k, &(a, b)) in left.iter().enumerate() {
        let piece = if k == 0 {
            id.clone()
        } else {
            let copy = ItemId::new();
            ops.push(ItemOp::Copy {
                item: id.clone(),
                into: copy.clone(),
            });
            copy
        };
        ops.push(ItemOp::Place {
            item: piece.clone(),
            at: a,
            length: b - a,
            offset: offset + (a - start),
        });
        let own =
            |edge: f64, at: f64, fade: f64| if (edge - at).abs() < GRAIN { fade } else { 0.0 };
        ops.push(ItemOp::Fades {
            item: piece,
            fade_in: own(start, a, item.fade_in_length.as_seconds()),
            fade_out: own(end, b, item.fade_out_length.as_seconds()),
        });
    }
}

/// `item`'s audio over `from..to` copied onto `lane`, with the comp's short
/// fade either end.
fn copy_ops(item: &daw_proto::Item, (from, to): (f64, f64), lane: u32, ops: &mut Vec<ItemOp>) {
    let start = item.position.as_seconds();
    let (a, b) = (from.max(start), to.min(start + item.length.as_seconds()));
    if b - a <= 1e-6 {
        return;
    }
    let copy = ItemId::new();
    ops.extend([
        ItemOp::Copy {
            item: ItemId::from(item.guid.as_str()),
            into: copy.clone(),
        },
        ItemOp::Lane {
            item: copy.clone(),
            lane,
        },
        ItemOp::Place {
            item: copy.clone(),
            at: a,
            length: b - a,
            offset: item.start_offset.as_seconds() + (a - start),
        },
        ItemOp::Fades {
            item: copy,
            fade_in: crate::comp::JOIN,
            fade_out: crate::comp::JOIN,
        },
    ]);
}

static BEST: ActionMeta = crate::actions::meta(
    "COMP_BEST",
    "comp_best",
    "Comp the best takes",
    "Comp the kit from its best takes over a stretch — `from`/`to`, `spans` or `sections`; none: the whole song — as one edit: each section cut into phrases of `phrase_bars` bars (4) — the stretches kept as comped (COMP_KEEP) left out; with a main take (COMP_MAIN) a phrase stays on it unless a take alike to it scores 0.25 better — every phrase ranked from the takes' hits against the part as written (COMP_CANDIDATES's ranking), and those whose best judged take is not the one playing comped from it — every mic from its own lane, the EDIT lane there the new comp as recorded — in one undo step. `dry_run` says what it would comp. What Ctrl+T does over the selection, or the whole song with none.",
    "Edit",
    "",
);

/// Where to comp, and how finely.
#[derive(Facet)]
struct BestArg {
    from: Option<f64>,
    to: Option<f64>,
    #[facet(default)]
    spans: Vec<editing::scope::Span>,
    #[facet(default)]
    sections: Vec<String>,
    phrase_bars: Option<f64>,
    dry_run: Option<bool>,
}

/// What it comped.
#[derive(Facet)]
struct BestOut {
    phrases: u32,
    comped: Vec<ChoiceOut>,
    ops: u32,
}

#[derive(Facet)]
struct ChoiceOut {
    from: f64,
    to: f64,
    take: String,
    score: f64,
}

pub(crate) fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&BEST, Effect::Mutating, |a: BestArg| {
        let song = Song::current()?;
        let bars = a.phrase_bars.unwrap_or(PHRASE_BARS);
        let spans = stretches(&song, &a);
        let phrases: Vec<(f64, f64)> = spans
            .into_iter()
            .flat_map(|span| phrases(&song, span, bars))
            .collect();
        // What is kept as comped is not chosen again.
        let kept = crate::comp_keep::kept(song.daw(), &song.context());
        let phrases = crate::comp_keep::outside(&phrases, &kept);
        let done = comp_best(&song, &phrases, a.dry_run.unwrap_or(false))?;
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Ok(BestOut {
            phrases: count(done.phrases),
            ops: count(done.ops),
            comped: done
                .choices
                .into_iter()
                .map(|c| ChoiceOut {
                    from: c.from,
                    to: c.to,
                    take: c.take,
                    score: c.score,
                })
                .collect(),
        })
    })?;
    Ok(())
}

/// The stretches a call names: its spans, its `from`/`to`, its sections —
/// else the whole song (every section).
fn stretches(song: &Song, a: &BestArg) -> Vec<(f64, f64)> {
    let sections = crate::actions::sections(song.daw(), &song.context());
    let mut out: Vec<(f64, f64)> = a.spans.iter().map(|s| (s.from, s.to)).collect();
    if let (Some(from), Some(to)) = (a.from, a.to) {
        out.push((from.min(to), from.max(to)));
    }
    out.extend(
        sections
            .iter()
            .filter(|s| a.sections.iter().any(|n| n.eq_ignore_ascii_case(&s.name)))
            .map(|s| (s.start, s.end)),
    );
    if out.is_empty()
        && let (Some(first), Some(last)) = (sections.first(), sections.last())
    {
        out.push((first.start, last.end));
    }
    union(out.into_iter())
}
