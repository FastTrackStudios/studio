//! The comp's seams placed where they are heard least
//! ([`editing::seam`]), on the song's own audio.
//!
//! A seam goes just before an attack — of any take — so the attack
//! covers the switch. Which attack is chosen by listening: each mic of
//! the kit read over a stretch around the seam, as the take before it
//! plays and as the take after it does, every candidate cut scored by
//! what changes there over what covers it. A ghost note or a stretch of
//! ringing cymbals loses to a backbeat.

use daw::standalone::sync::ProjectState;
use daw_proto::ProjectContext;

use crate::hit_place::Slice;

/// How far a seam may move from where it was asked for, seconds: about a
/// beat at most tempos.
pub(crate) const SEARCH: f64 = 0.4;
/// Around the stretch the candidates are in, how much more audio is read,
/// seconds: room for what each is compared over.
const MARGIN: f64 = 0.1;

/// One mic either side of a seam: its track, the lane playing before the
/// seam and the lane playing after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pair {
    pub track: String,
    pub out: u32,
    pub incoming: u32,
}

/// A seam where it is heard least, song seconds, and how much.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placed {
    pub at: f64,
    pub heard: f64,
    /// How much a cut where it was asked for is heard.
    pub heard_there: f64,
}

/// Where a seam asked for at `wanted` goes: of `candidates` (song
/// seconds) inside `allowed`, the cut heard least across every mic of
/// `pairs`, nearer `wanted` between two alike. `None` when no mic reads.
pub(crate) fn place(
    p: &ProjectState,
    pairs: &[Pair],
    candidates: &[f64],
    (wanted, allowed): (f64, (f64, f64)),
) -> Option<Placed> {
    let window = (
        allowed.0.min(wanted) - MARGIN,
        allowed.1.max(wanted) + MARGIN,
    );
    let read: Vec<(Vec<f32>, Vec<f32>, f64)> = pairs
        .iter()
        .filter_map(|pair| {
            let (out, rate) = lane_audio(p, &pair.track, pair.out, window)?;
            let (incoming, theirs) = lane_audio(p, &pair.track, pair.incoming, window)?;
            ((rate - theirs).abs() < f64::EPSILON).then_some((out, incoming, rate))
        })
        .collect();
    let rate = read.first()?.2;
    let mics: Vec<editing::seam::Mic<'_>> = read
        .iter()
        .filter(|(_, _, r)| (r - rate).abs() < f64::EPSILON)
        .map(|(out, incoming, _)| editing::seam::Mic { out, incoming })
        .collect();
    let inside: Vec<f64> = candidates
        .iter()
        .chain(std::iter::once(&wanted))
        .filter(|c| **c >= allowed.0 && **c <= allowed.1)
        .map(|c| c - window.0)
        .collect();
    let reach = (wanted - allowed.0).max(allowed.1 - wanted).max(1e-3);
    let best = editing::seam::best(&mics, rate, &inside, (wanted - window.0, reach))?;
    Some(Placed {
        at: best.at + window.0,
        heard: best.heard,
        heard_there: editing::seam::heard_at(&mics, rate, wanted - window.0),
    })
}

/// What `lane` of `track` plays over `from..to`, mono, at its
/// recordings' rate — silence where it plays nothing. `None` when no
/// recording there reads.
fn lane_audio(
    p: &ProjectState,
    track: &str,
    lane: u32,
    (from, to): (f64, f64),
) -> Option<(Vec<f32>, f64)> {
    let slices = crate::hit_place::lane_of(p, track, lane, (from, to));
    let mut out: Option<(Vec<f32>, f64)> = None;
    for slice in &slices {
        let Some(((audio, rate), at)) = slice_audio(slice, (from, to)) else {
            continue;
        };
        let (buffer, r) = out.get_or_insert_with(|| {
            let frames = num_traits::ToPrimitive::to_usize(&((to - from) * rate).round());
            (vec![0.0; frames.unwrap_or(0)], rate)
        });
        if (*r - rate).abs() > f64::EPSILON {
            continue;
        }
        for (slot, s) in buffer.iter_mut().skip(at).zip(audio) {
            *slot = s;
        }
    }
    out
}

/// The part of `slice` inside `from..to`, read from its recording, and
/// the frame of the stretch it starts at.
fn slice_audio(slice: &Slice, (from, to): (f64, f64)) -> Option<((Vec<f32>, f64), usize)> {
    let file = slice.file.as_deref()?;
    let (a, b) = (slice.start().max(from), slice.end().min(to));
    if b <= a {
        return None;
    }
    let (audio, rate) = crate::audio_window::read_seconds(file, slice.source_at(a), b - a)?;
    let at = num_traits::ToPrimitive::to_usize(&((a - from) * rate).round())?;
    Some(((audio, rate), at))
}

/// Each mic of `lanes` (a take's, `(track, lane)`) paired with its comp
/// lane: `before` the take comes in (`true`) the comp is what plays, after
/// it goes out the take is.
pub(crate) fn pairs(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    lanes: &[(String, u32)],
    comp_first: bool,
) -> Vec<Pair> {
    let tracks = daw::service::Tracks::all(daw, project.clone());
    lanes
        .iter()
        .filter_map(|(track, lane)| {
            let t = tracks.iter().find(|t| t.guid == *track)?;
            let comp = crate::comp::comp_lane_of(daw, project, t)?;
            let (out, incoming) = if comp_first {
                (comp, *lane)
            } else {
                (*lane, comp)
            };
            Some(Pair {
                track: track.clone(),
                out,
                incoming,
            })
        })
        .collect()
}

/// Two spans of the comp this close end to start meet at a seam, seconds.
const MEETS: f64 = 0.02;
/// A seam moved less than this stays where it is, seconds.
const STAYS: f64 = 0.001;
/// How much quieter a seam must be somewhere else to move there.
const BETTER: f64 = 0.8;
/// A seam is kept this far from the seams either side of it, seconds.
const APART: f64 = 0.05;

/// One seam of the comp: where it is, the takes either side, and where
/// it is heard least.
#[derive(Clone, Debug)]
pub(crate) struct Surveyed {
    pub at: f64,
    pub before: crate::hits::TakeVersion,
    pub after: crate::hits::TakeVersion,
    /// Where it is heard least, and how much; how much where it is.
    pub placed: Placed,
}

impl Surveyed {
    /// Whether it is enough better somewhere else to move: a little
    /// quieter is not worth more of the other take.
    #[must_use]
    pub(crate) fn moves(&self) -> bool {
        (self.placed.at - self.at).abs() >= STAYS
            && self.placed.heard < BETTER * self.placed.heard_there
    }
}

/// Every seam of the comp over `from..to` — where the kit switches from
/// one take to another — with where it is heard least.
pub(crate) fn survey(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (from, to): (f64, f64),
) -> Vec<Surveyed> {
    let (kit, _) = crate::hits::kit(daw, project);
    let comps = crate::hits::comps(daw, project, &kit);
    let seams: Vec<f64> = comps
        .windows(2)
        .filter(|w| w[0].take != w[1].take && w[1].start - w[0].end <= MEETS)
        .map(|w| w[1].start)
        .filter(|s| *s >= from && *s <= to)
        .collect();
    // A fill's seams, and a kept stretch's, move only out of it.
    let fills: Vec<(f64, f64)> = crate::hits::zones(daw, project)
        .into_iter()
        .filter(|z| z.kind == crate::hits::ZoneKind::Fill)
        .map(|z| (z.start, z.end))
        .chain(
            crate::comp_keep::kept(daw, project)
                .into_iter()
                .map(|k| (k.from, k.to)),
        )
        .collect();
    let Some(song) = crate::hit_index::song_of(project) else {
        return Vec::new();
    };
    seams
        .iter()
        .enumerate()
        .filter_map(|(i, &at)| {
            let prev = i
                .checked_sub(1)
                .and_then(|p| seams.get(p))
                .map_or(f64::NEG_INFINITY, |p| p + APART);
            let next = seams.get(i + 1).map_or(f64::INFINITY, |n| n - APART);
            let allowed = allowed_around(
                at,
                &fills,
                ((at - SEARCH).max(prev), (at + SEARCH).min(next)),
            );
            let versions = crate::comp_rank::versions_over(daw, project, allowed);
            let side = |span: (f64, f64)| {
                let pass = crate::comp_rank::current(daw, project, &versions, span)?;
                versions.iter().find(|v| v.pass == pass).cloned()
            };
            let before = side((at - 0.02, at - 0.01))?;
            let after = side((at + 0.01, at + 0.02))?;
            let pairs = pairs_of(&before, &after);
            let cuts: Vec<f64> = versions
                .iter()
                .flat_map(|v| v.hits.iter().map(|h| h.1 - crate::comp_song::SEAM_LEAD))
                .collect();
            let placed = song.read(|p| place(p, &pairs, &cuts, (at, allowed)))??;
            Some(Surveyed {
                at,
                before,
                after,
                placed,
            })
        })
        .collect()
}

/// How near a fill's edge a seam is its seam, seconds: a fill comped from
/// a take begins and ends just before the hits at its edges.
const FILL_EDGE: f64 = 0.15;

/// Where the seam at `at` may go within `around`: never into a fill
/// whose seam it is — the take chosen for the fill keeps all of it — and
/// out of it no further than [`SEARCH`] from the fill's edge, however
/// often the seams are placed.
fn allowed_around(at: f64, fills: &[(f64, f64)], around: (f64, f64)) -> (f64, f64) {
    fills.iter().fold(around, |(lo, hi), &(start, end)| {
        if at <= start + FILL_EDGE && at >= start - SEARCH - FILL_EDGE {
            (lo.max(start - SEARCH), hi.min(at.max(start - SEARCH)))
        } else if at >= end - FILL_EDGE && at <= end + SEARCH + FILL_EDGE {
            (lo.max(at.min(end + SEARCH)), hi.min(end + SEARCH))
        } else {
            (lo, hi)
        }
    })
}

/// Each mic both takes have, paired lane to lane.
fn pairs_of(before: &crate::hits::TakeVersion, after: &crate::hits::TakeVersion) -> Vec<Pair> {
    before
        .lanes
        .iter()
        .filter_map(|(track, out)| {
            let incoming = after.lanes.iter().find(|(t, _)| t == track)?.1;
            Some(Pair {
                track: track.clone(),
                out: *out,
                incoming,
            })
        })
        .collect()
}

/// Each seam of `seams` that is better elsewhere moved there: the stretch
/// between comped from the take that now plays it, and the kit's edit
/// made again over it. One undo step. How many moved.
///
/// # Errors
///
/// The engine refusing an edit.
pub(crate) fn apply(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    seams: &[Surveyed],
) -> Result<usize, String> {
    let moving: Vec<&Surveyed> = seams.iter().filter(|s| s.moves()).collect();
    if moving.is_empty() {
        return Ok(0);
    }
    let label = "Place the comp's seams where they are heard least";
    daw::service::Projects::begin_undo_block(daw, project.clone(), label);
    let done = moving.iter().try_for_each(|s| {
        let (span, take) = if s.placed.at < s.at {
            ((s.placed.at, s.at), &s.after)
        } else {
            ((s.at, s.placed.at), &s.before)
        };
        crate::comp::swipe_lanes(daw, project, &take.lanes, span, label)?;
        // As a comp from the take leaves them: the mics it has no
        // recording on, silent.
        let versions = crate::comp_rank::versions_over(daw, project, span);
        crate::comp::silence(daw, project, &take.unrecorded(&versions), span)
            .map_err(|e| e.to_string())?;
        let tracks: Vec<String> = take.lanes.iter().map(|(t, _)| t.clone()).collect();
        crate::comp::edit_from_comp(daw, project, &tracks, span).map(|_| ())
    });
    daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
    done?;
    crate::comp_rank::forget();
    crate::studio::request_resync_items();
    crate::hit_overlay::refresh();
    Ok(moving.len())
}

static SEAMS: architect::action::ActionMeta = crate::actions::meta(
    "COMP_SEAMS",
    "comp_seams",
    "Comp seams where they are heard least",
    "Every seam of the kit's comp — where it switches from one take to another — and where it is heard least: just before an attack of any take within 0.4 s (never past the seams either side), the one that covers the switch best, every mic of the kit compared as the take before and the take after play there (how much the two disagree across the cut and how much the wash under it jumps, over how loud what follows is). Per seam: `at`, the takes `before`/`after`, `heard` there (about 1: as loud as what covers it), `best_at` and `heard_best`, and `moves`. Unless `dry_run`, each that is better elsewhere is moved there — the stretch between comped from the take that now plays it and the kit's edit made again over it — as one undo step. `from`/`to` (seconds) narrow it.",
    "Comp",
    "",
);

/// Over when, and whether to only say.
#[derive(facet::Facet)]
struct SeamsArg {
    from: Option<f64>,
    to: Option<f64>,
    dry_run: Option<bool>,
}

/// One seam.
#[derive(facet::Facet)]
struct SeamOut {
    at: f64,
    before: String,
    after: String,
    heard: f64,
    best_at: f64,
    heard_best: f64,
    moves: bool,
}

/// What placing the seams found and did.
#[derive(facet::Facet)]
struct SeamsOut {
    seams: Vec<SeamOut>,
    moved: u32,
}

pub(crate) fn register(
    list: &architect::action::ActionList,
) -> Result<(), architect::action::ActionError> {
    list.register(
        &SEAMS,
        architect::action::Effect::Mutating,
        |a: SeamsArg| {
            let (daw, project) = crate::actions::song()?;
            let span = (
                a.from.unwrap_or(f64::NEG_INFINITY),
                a.to.unwrap_or(f64::INFINITY),
            );
            let seams = survey(daw, &project, span);
            let moved = if a.dry_run.unwrap_or(false) {
                0
            } else {
                apply(daw, &project, &seams)?
            };
            let round = |v: f64| (v * 1_000.0).round() / 1_000.0;
            let name =
                |v: &crate::hits::TakeVersion| v.lane.clone().unwrap_or_else(|| v.pass.clone());
            Ok(SeamsOut {
                seams: seams
                    .iter()
                    .map(|s| SeamOut {
                        at: round(s.at),
                        before: name(&s.before),
                        after: name(&s.after),
                        heard: round(s.placed.heard_there),
                        best_at: round(s.placed.at),
                        heard_best: round(s.placed.heard),
                        moves: s.moves(),
                    })
                    .collect(),
                moved: u32::try_from(moved).unwrap_or(u32::MAX),
            })
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fill's seams move out of it, never in; another seam either way.
    #[test]
    fn a_fills_seams_only_move_out_of_it() {
        let fills = [(10.0, 12.0)];
        assert_eq!(allowed_around(9.99, &fills, (9.59, 10.39)), (9.6, 9.99));
        assert_eq!(allowed_around(12.01, &fills, (11.61, 12.41)), (12.01, 12.4));
        assert_eq!(allowed_around(20.0, &fills, (19.6, 20.4)), (19.6, 20.4));
        // Moved out once, it is still the fill's: no further out than the
        // reach from the fill's edge.
        assert_eq!(allowed_around(9.7, &fills, (9.3, 10.1)), (9.6, 9.7));
        assert_eq!(allowed_around(9.6, &fills, (9.2, 10.0)), (9.6, 9.6));
    }
}
