//! Comping a stretch from the take that plays it best: `COMP_CANDIDATES`,
//! `COMP_CYCLE` and `COMP_CHOOSE`.
//!
//! The kit's basics are quantized; what is left is choosing, fill by fill,
//! the take the drummer played it best in. Each take's version of the
//! stretch — the selection, a fill's hits, a section — is ranked
//! ([`editing::comp::rank`]): whether it plays it and how tight it is —
//! the drum MIDI says where a hit belongs, not which fill to play. T and Shift+T step through them, best
//! first (`COMP_CYCLE`); a model reads the ranking (`COMP_CANDIDATES`) and
//! nominates one (`COMP_CHOOSE`). A take is comped onto the kit as one —
//! every mic from the same take ([`crate::comp::swipe`] on the Drums
//! folder) — and where the kit is edited (its `EDIT` lanes), the edit over
//! the stretch is made the new comp as recorded, so what plays is the new
//! take.

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{Items as _, ProjectContext, Tracks};
use editing::comp::{Candidate, Ranked, Strike};
use facet::Facet;

static CANDIDATES: ActionMeta = crate::actions::meta(
    "COMP_CANDIDATES",
    "comp_candidates",
    "Rank the takes for a stretch",
    "Every take of the kit ranked for a stretch — the selection (a fill's hit markers, rows, a razor, a time selection) or `from`/`to` / `sections` — from its hit markers: whether it plays the stretch at all and how tight it is with its own feel taken out (`spread_ms`, `feel_ms`, against the drum MIDI's notes where they are near, else the grid) make its `score`, best first — what it plays is the drummer's choice, so how much of the drum MIDI there it plays (`coverage`; with no MIDI, how near its count of hits is to the takes' usual) is reported, not scored; `current` marks the take the comp plays there now; `color` is the take's colour as the window paints its lane and comp pieces (`Blue`, `Light Green`); `unplayable` marks a take whose lanes hold no audio over all of the stretch (a punch-in's pre-roll, recorded but cut away), ranked last and not comped from. Read-only: what COMP_CYCLE steps through and COMP_CHOOSE takes from.",
    "Edit",
    "",
);

static CYCLE: ActionMeta = crate::actions::meta(
    "COMP_CYCLE",
    "comp_cycle",
    "Comp the next take",
    "Over a stretch (the selection, or `from`/`to` / `sections`), comp the kit from the next take in COMP_CANDIDATES' ranking (`by` 1, the default) or the one before (-1), round from the last to the first, empty takes skipped; from none comped there, the best. Every mic from the same take; where the kit is edited (its EDIT lanes), the edit over the stretch is made the new comp as recorded. Says the take, the ranking and the stretch. T and Shift+T.",
    "Edit",
    "T",
);

static CHOOSE: ActionMeta = crate::actions::meta(
    "COMP_CHOOSE",
    "comp_choose",
    "Comp a take",
    "Over a stretch (the selection, or `from`/`to` / `sections`), comp the kit from take `take` — its pass, its lane's name or its colour (`blue`, `light green`), as COMP_CANDIDATES says them — every mic from it; where the kit is edited, the edit over the stretch made the new comp as recorded. What a model nominates as the best version of a fill.",
    "Edit",
    "",
);

/// Which stretch.
#[derive(Facet)]
struct CandidatesArg {
    #[facet(flatten, default)]
    on: crate::edit_actions::On,
}

/// Which way.
#[derive(Facet)]
struct CycleArg {
    #[facet(flatten, default)]
    on: crate::edit_actions::On,
    by: Option<i32>,
}

/// Which take.
#[derive(Facet)]
struct ChooseArg {
    #[facet(flatten, default)]
    on: crate::edit_actions::On,
    take: String,
}

/// One take, as ranked for a stretch.
#[derive(Facet, Clone, Debug)]
pub(crate) struct TakeRank {
    /// Which take: its recording pass, what COMP_CHOOSE takes.
    pub(crate) take: String,
    pub(crate) pass: String,
    /// Its lane's name on the first mic that has it: what to call it.
    pub(crate) lane: String,
    pub(crate) score: f64,
    pub(crate) hits: u32,
    pub(crate) coverage: f64,
    pub(crate) spread_ms: f64,
    pub(crate) feel_ms: f64,
    pub(crate) empty: bool,
    /// Its hits were never found (HITS_DETECT with `all_takes`): not
    /// judged, ranked after those that were.
    pub(crate) undetected: bool,
    /// The comp plays this take there now.
    pub(crate) current: bool,
    /// Its lanes hold no audio over all of the stretch — a punch-in's
    /// pre-roll, recorded but cut away: its hits are heard nowhere, and a
    /// comp from it would leave a hole. Ranked last; COMP_CHOOSE refuses it.
    pub(crate) unplayable: bool,
    /// Its colour's name, as the window paints its lane and its comp
    /// pieces ([`crate::take_colors`]): "the blue take". What COMP_CHOOSE
    /// takes as well as its pass.
    pub(crate) color: Option<String>,
}

/// A stretch's ranking.
#[derive(Facet)]
struct Ranking {
    from: f64,
    to: f64,
    takes: Vec<TakeRank>,
    /// What to do before the ranking can judge every take, if anything.
    note: Option<String>,
}

/// What a comp did.
#[derive(Facet)]
struct Comped {
    from: f64,
    to: f64,
    /// The take comped in.
    take: String,
    takes: Vec<TakeRank>,
    /// The edit over the stretch made the new comp as recorded.
    edit_restored: bool,
}

/// The drum MIDI over `from..to` as strikes on the kit's pieces (its
/// pitches learned against the hits); none without a drum MIDI track.
///
/// Which pitch is which piece is learned against the takes' own hits as
/// recorded (`versions`) — never against where the comp plays them now,
/// which would rank a take differently once the comp around it changed.
pub(crate) fn written(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (from, to): (f64, f64),
    versions: &[crate::hits::TakeVersion],
) -> Vec<Strike> {
    let Some(midi) = crate::drum_reference::find_track(daw, project) else {
        return Vec::new();
    };
    let notes = crate::drum_reference::notes(daw, project, &midi);
    let mut by_piece: std::collections::BTreeMap<&str, Vec<f64>> =
        std::collections::BTreeMap::new();
    for (piece, t) in versions.iter().flat_map(|v| v.hits.iter()) {
        by_piece.entry(piece.as_str()).or_default().push(*t);
    }
    let named: Vec<(String, Vec<f64>)> = by_piece
        .into_iter()
        .map(|(piece, mut times)| {
            times.sort_by(f64::total_cmp);
            (piece.to_owned(), times)
        })
        .collect();
    let map = crate::drum_reference::learn_map(&notes, &named, 0.030);
    notes
        .iter()
        .filter(|n| n.time >= from && n.time < to)
        .filter_map(|n| {
            let piece = map.iter().find(|m| m.pitch == n.pitch)?.piece.clone()?;
            Some(Strike { piece, t: n.time })
        })
        .collect()
}

/// The take the comp plays over the middle of `from..to`: the one whose
/// recording a COMP lane item there plays.
fn current(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    versions: &[crate::hits::TakeVersion],
    (from, to): (f64, f64),
) -> Option<String> {
    let mid = (from + to) / 2.0;
    let tracks = Tracks::all(daw, project.clone());
    versions.iter().find_map(|v| {
        let hit = v.files.iter().any(|(track, path)| {
            let Some(t) = tracks.iter().find(|t| t.guid == *track) else {
                return false;
            };
            let comp = crate::drum_lanes::lane_named(t, crate::drum_lanes::COMP).unwrap_or(0);
            daw.get_items(project.clone(), daw::service::TrackRef::Guid(track.clone()))
                .into_iter()
                .filter(|i| i.fixed_lane.unwrap_or(0) == comp)
                .filter(|i| {
                    let at = i.position.as_seconds();
                    mid >= at && mid < at + i.length.as_seconds()
                })
                .any(|i| {
                    daw::service::Takes::get_active_take(
                        daw,
                        project.clone(),
                        daw::service::ItemRef::Guid(i.guid),
                    )
                    .and_then(|take| take.source_file_path)
                    .is_some_and(|p| p == *path)
                })
        });
        hit.then(|| v.pass.clone())
    })
}

/// Each take in `out` whose lanes hold no audio over all of `from..to`
/// marked so, and put last.
fn mark_unplayable(
    out: &mut [TakeRank],
    project: &ProjectContext,
    versions: &[crate::hits::TakeVersion],
    span: (f64, f64),
) {
    let Some(song) = crate::hit_index::song_of(project) else {
        return;
    };
    let audio = crate::comp_song::Audio::read(&song, versions, span);
    for take in out.iter_mut() {
        take.unplayable = versions
            .iter()
            .find(|v| v.pass == take.pass)
            .is_some_and(|v| !audio.covers(v, span));
    }
    out.sort_by_key(|t| u8::from(t.unplayable));
}

/// The takes ranked over `from..to`, and which the comp plays there.
fn ranking(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (from, to): (f64, f64),
) -> (Vec<TakeRank>, Vec<crate::hits::TakeVersion>) {
    let versions = versions_over(daw, project, (from, to));
    let now = current(daw, project, &versions, (from, to));
    let written = written(daw, project, (from, to), &versions);
    let line_of = |t: f64| crate::actions::grid_line(daw, project, t, 16);
    let mut out = rank_versions(&versions, &written, now.as_deref(), &line_of);
    mark_unplayable(&mut out, project, &versions, (from, to));
    name_colors(&mut out, &kit_takes(project));
    (out, versions)
}

/// Each take ranked given its colour's name.
fn name_colors(ranked: &mut [TakeRank], takes: &crate::take_colors::TakeColors) {
    for take in ranked {
        take.color = takes.name_of(&take.pass);
    }
}

/// The kit's takes, numbered as the window numbers them: every take named
/// on the kit's folder and the tracks under it.
fn kit_takes(project: &ProjectContext) -> crate::take_colors::TakeColors {
    let Some(song) = crate::hit_index::song_of(project) else {
        return crate::take_colors::TakeColors::default();
    };
    song.read(|p| {
        let tracks = &p.tracks;
        // The top folder over the kit's first piece, as `track_regions::kit_folder` finds it.
        let (_, piece) = crate::drum_reference::pieces_of(tracks)
            .into_iter()
            .next()?;
        let mut folder = tracks.iter().find(|t| t.guid == piece)?;
        while let Some(parent) = folder
            .parent_guid
            .as_deref()
            .and_then(|g| tracks.iter().find(|t| t.guid == g))
        {
            folder = parent;
        }
        let under = daw_ui::studio::folded::under(tracks, &folder.guid);
        // As the window counts them: each item's take name and length.
        let mut longest: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
        for guid in std::iter::once(folder)
            .chain(under)
            .flat_map(|t| p.items_by_track.get(&t.guid).into_iter().flatten())
        {
            let Some(list) = p.takes.get(guid) else {
                continue;
            };
            let (Some(take), Some(item)) = (
                usize::try_from(list.active_idx)
                    .ok()
                    .and_then(|i| list.takes.get(i)),
                p.items.get(guid),
            ) else {
                continue;
            };
            let at = longest.entry(take.name.as_str()).or_insert(0.0);
            *at = at.max(item.item.length.as_seconds());
        }
        Some(crate::take_colors::TakeColors::of(longest))
    })
    .flatten()
    .unwrap_or_default()
}

/// `versions` (each with its hits over the stretch) ranked against the
/// part as `written` there: the judged first, best first; then the
/// unjudged; the empty last. `now` is the take the comp plays there.
pub(crate) fn rank_versions(
    versions: &[crate::hits::TakeVersion],
    written: &[Strike],
    now: Option<&str>,
    line_of: &dyn Fn(f64) -> f64,
) -> Vec<TakeRank> {
    let candidates: Vec<Candidate> = versions
        .iter()
        .map(|v| Candidate {
            take: v.pass.clone(),
            hits: v
                .hits
                .iter()
                .map(|(piece, t)| Strike {
                    piece: piece.clone(),
                    t: *t,
                })
                .collect(),
        })
        .collect();
    let lane_of = |pass: &str| {
        versions
            .iter()
            .find(|v| v.pass == pass)
            .and_then(|v| v.lane.clone())
            .unwrap_or_default()
    };
    let undetected = |pass: &str| {
        versions
            .iter()
            .find(|v| v.pass == pass)
            .is_some_and(|v| !v.detected)
    };
    let mut out: Vec<TakeRank> = editing::comp::rank(&candidates, written, line_of)
        .into_iter()
        .map(|r: Ranked| {
            let never = undetected(&r.take);
            TakeRank {
                current: now == Some(r.take.as_str()),
                lane: lane_of(&r.take),
                pass: r.take.clone(),
                // Not found is not empty: unjudged, still to listen to.
                empty: r.empty && !never,
                undetected: never,
                take: r.take,
                score: r.score,
                hits: u32::try_from(r.hits).unwrap_or(u32::MAX),
                coverage: r.coverage,
                spread_ms: r.spread_ms,
                feel_ms: r.feel_ms,
                color: None,
                unplayable: false,
            }
        })
        .collect();
    out.sort_by_key(|t| u8::from(t.undetected) + 2 * u8::from(t.empty));
    out
}

/// Every take of the kit over `from..to`, one a recording pass: a pass
/// recorded from more than one place in the song is one take, its
/// recordings' hits merged (by time), each mic's lanes with them.
pub(crate) fn versions_over(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (from, to): (f64, f64),
) -> Vec<crate::hits::TakeVersion> {
    let mut versions: Vec<crate::hits::TakeVersion> = Vec::new();
    for v in crate::hits::takes_over(daw, project, (from, to)) {
        if v.lanes.is_empty() {
            continue;
        }
        match versions.iter_mut().find(|w| w.pass == v.pass) {
            Some(w) => {
                w.hits.extend(v.hits);
                w.files.extend(v.files);
                w.lanes.extend(v.lanes);
                w.detected |= v.detected;
            }
            None => versions.push(v),
        }
    }
    for v in &mut versions {
        v.hits.sort_by(|a, b| a.1.total_cmp(&b.1));
    }
    versions
}

/// `from..to` with each end moved to just before the hits at it, every
/// take's ([`crate::comp_song::clear_seam`]): a fill's end on the downbeat
/// cut before both takes' attacks there, not between them.
fn clear_of_hits(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (from, to): (f64, f64),
) -> (f64, f64) {
    let reach = crate::comp_song::SEAM_REACH;
    let attacks = |at: f64| {
        let mut all: Vec<f64> = versions_over(daw, project, (at - reach, at + reach))
            .into_iter()
            .flat_map(|v| v.hits.into_iter().map(|h| h.1))
            .collect();
        all.sort_by(f64::total_cmp);
        all
    };
    let start = crate::comp_song::clear_seam(from, &attacks(from));
    let end = crate::comp_song::clear_seam(to, &attacks(to));
    if end - start > crate::comp::GRAIN {
        (start, end)
    } else {
        (from, to)
    }
}

/// The kept ranking forgotten: the comp changed under it.
pub(crate) fn forget() {
    if let Ok(mut last) = LAST.lock() {
        *last = None;
    }
}

/// Comp the kit from `take` over `from..to` — each mic from its own lane
/// holding its recording of the take, the mics it never recorded (of
/// `versions`) silent there — and where it is edited, its edit there made
/// the new comp as recorded. Whether the edit was.
fn comp_in(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (take, versions): (&crate::hits::TakeVersion, &[crate::hits::TakeVersion]),
    (from, to): (f64, f64),
) -> Result<bool, String> {
    let mut stages = architect_telemetry::timing::Stages::start();
    let (from, to) = clear_of_hits(daw, project, (from, to));
    let label = format!("Comp from take {}", take.pass);
    daw::service::Projects::begin_undo_block(daw, project.clone(), &label);
    let comped =
        crate::comp::swipe_lanes(daw, project, &take.lanes, (from, to), &label).and_then(|n| {
            crate::comp::silence(daw, project, &take.unrecorded(versions), (from, to))
                .map(|_| n)
                .map_err(|e| e.to_string())
        });
    stages.mark("comp.swipe_ms");
    // Where the kit is edited, its edit over the stretch made the new comp
    // as recorded — on the engine, in the same undo step, every mic.
    let tracks: Vec<String> = take.lanes.iter().map(|(t, _)| t.clone()).collect();
    let edited = comped
        .as_ref()
        .ok()
        .map(|_| crate::comp::edit_from_comp(daw, project, &tracks, (from, to)))
        .transpose();
    stages.mark("comp.restore_ms");
    daw::service::Projects::end_undo_block(daw, project.clone(), &label, None);
    comped?;
    let edited = edited?.is_some_and(|n| n > 0);
    // Only the mics' items changed (their comp and edit lanes): the window
    // reads back those, as the engine's journal names them.
    crate::studio::request_resync_items();
    crate::hit_overlay::refresh();
    Ok(edited)
}

/// The last stretch ranked, its ranking, its takes, and the take last
/// comped there: T pressed again on the same stretch steps from it at
/// once, rather than ranking every take again.
struct Kept {
    span: (f64, f64),
    takes: Vec<TakeRank>,
    versions: Vec<crate::hits::TakeVersion>,
    current: Option<String>,
}

static LAST: std::sync::Mutex<Option<Kept>> = std::sync::Mutex::new(None);

/// The stretch's ranking: the one kept when it is the same stretch, else
/// ranked now (and kept).
fn ranked_for(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    span: (f64, f64),
) -> (Vec<TakeRank>, Vec<crate::hits::TakeVersion>, Option<String>) {
    let same = |s: (f64, f64)| (s.0 - span.0).abs() < 1e-3 && (s.1 - span.1).abs() < 1e-3;
    if let Ok(last) = LAST.lock()
        && let Some(kept) = last.as_ref().filter(|k| same(k.span))
    {
        return (
            kept.takes.clone(),
            kept.versions.clone(),
            kept.current.clone(),
        );
    }
    let (takes, versions) = ranking(daw, project, span);
    let current = takes.iter().find(|t| t.current).map(|t| t.take.clone());
    if let Ok(mut last) = LAST.lock() {
        *last = Some(Kept {
            span,
            takes: takes.clone(),
            versions: versions.clone(),
            current: current.clone(),
        });
    }
    (takes, versions, current)
}

/// The take comped last on `span`, kept for the next press.
fn now_comped(span: (f64, f64), take: &str) {
    if let Ok(mut last) = LAST.lock()
        && let Some(kept) = last.as_mut()
        && (kept.span.0 - span.0).abs() < 1e-3
        && (kept.span.1 - span.1).abs() < 1e-3
    {
        kept.current = Some(take.to_owned());
        for t in &mut kept.takes {
            t.current = t.take == take;
        }
    }
}

pub fn register(list: &ActionList) {
    list.register(&CANDIDATES, Effect::ReadOnly, |a: CandidatesArg| {
        let (daw, project) = crate::actions::song()?;
        Ok(crate::edit_actions::stretches(daw, &project, &a.on)?
            .into_iter()
            .map(|(from, to)| {
                let (takes, _) = ranking(daw, &project, (from, to));
                let unjudged = takes.iter().filter(|t| t.undetected).count();
                Ranking {
                    from,
                    to,
                    note: (unjudged > 0).then(|| {
                        format!(
                            "{unjudged} of {} takes have no hit markers: HITS_DETECT with `all_takes` finds them, once, so they can be judged",
                            takes.len()
                        )
                    }),
                    takes,
                }
            })
            .collect::<Vec<Ranking>>())
    });
    list.register(&CYCLE, Effect::Mutating, |a: CycleArg| {
        let (daw, project) = crate::actions::song()?;
        let mut stages = architect_telemetry::timing::Stages::start();
        let mut out = Vec::new();
        for (from, to) in crate::edit_actions::stretches(daw, &project, &a.on)? {
            let (takes, versions, now) = ranked_for(daw, &project, (from, to));
            stages.mark("comp.rank_ms");
            let ranked: Vec<Ranked> = takes
                .iter()
                .map(|t| Ranked {
                    take: t.take.clone(),
                    score: t.score,
                    hits: usize::try_from(t.hits).unwrap_or(0),
                    coverage: t.coverage,
                    spread_ms: t.spread_ms,
                    feel_ms: t.feel_ms,
                    // An unjudged take is still one to step to; one with
                    // no audio there is not.
                    empty: t.empty || t.unplayable,
                })
                .collect();
            let next = editing::comp::step(&ranked, now.as_deref(), a.by.unwrap_or(1))
                .ok_or("no take plays this stretch")?
                .take
                .clone();
            let version = versions
                .iter()
                .find(|v| v.pass == next)
                .ok_or("the take went")?;
            let edit_restored = comp_in(daw, &project, (version, &versions), (from, to))?;
            now_comped((from, to), &next);
            stages.mark("comp.comp_ms");
            out.push(Comped {
                from,
                to,
                take: next,
                takes,
                edit_restored,
            });
        }
        Ok(out)
    });
    list.register(&CHOOSE, Effect::Mutating, |a: ChooseArg| {
        let (daw, project) = crate::actions::song()?;
        let mut out = Vec::new();
        for (from, to) in crate::edit_actions::stretches(daw, &project, &a.on)? {
            let (takes, versions, _) = ranked_for(daw, &project, (from, to));
            // By its pass, its lane's name or its colour's, as
            // COMP_CANDIDATES says them.
            let colored = kit_takes(&project).pass_named(&a.take).map(str::to_owned);
            let version = versions
                .iter()
                .find(|v| {
                    v.pass == a.take
                        || v.lane.as_deref() == Some(a.take.as_str())
                        || colored.as_deref() == Some(v.pass.as_str())
                })
                .ok_or_else(|| format!("no take {} plays here", a.take))?;
            if takes.iter().any(|t| t.pass == version.pass && t.unplayable) {
                return Err(format!(
                    "take {} holds no audio over all of {from:.2}–{to:.2} s: a comp from it \
                     would leave a hole",
                    version.pass
                ));
            }
            let edit_restored = comp_in(daw, &project, (version, &versions), (from, to))?;
            now_comped((from, to), &version.pass);
            out.push(Comped {
                from,
                to,
                take: version.pass.clone(),
                takes,
                edit_restored,
            });
        }
        Ok(out)
    });
}
