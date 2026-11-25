//! The drum-edit planner: `daw.edit`'s `PLAN`.
//!
//! Where each played hit of a kit should go, worked out the way a
//! careful editor does it by hand:
//!
//! 1. **False doubles** out first: a trigger-only stroke just after its
//!    own drum's stroke and much quieter, a tom heard only on its mic
//!    beside a louder tom. They are no hit of their own (`remove`).
//! 2. **Seam doubles**: two takes sounding one stroke at a comp seam; the
//!    take that plays on keeps it, the other's sound goes (`delete`).
//! 3. **Rides**: a grace note rides its main stroke; strokes within
//!    [`UNISON`] of each other are one gesture, moved by its leader (the
//!    snare over the kick over the toms; for one drum, the first).
//! 4. **The lean**, per half bar: how early or late the drummer played,
//!    from the backbeat snare and the strokes on their own written note.
//! 5. **The grid per beat**: straight 16ths or triplets, from the written
//!    notes and the strokes' spacing (or a section's hint, or a fill's
//!    own grid).
//! 6. **Lines, per drum, in order**: a dynamic-programming fit of every
//!    leader to a line after the lean: never two strokes on one line,
//!    never out of order, a written note preferred within [`NOTE_WINDOW`],
//!    a straight run kept on consecutive lines.
//! 7. **Doubts**: only where the two best lines are close, or a move is
//!    long.

use std::collections::{BTreeMap, HashMap};

use daw_edit::PlanRequest;
use daw_edit::plan::{
    BarLine, Doubt, Heard, Lean, Plan, Planned, PlayedHit, Prefer, Reason, ZoneKind,
};

/// Strokes this close sound as one (a unison, a flam's second stroke).
pub const UNISON: f64 = 0.030;
/// A kick this soon before a snare is under it, and rides it.
pub const KICK_UNDER: f64 = 0.040;
/// Two drums' strokes this close were struck together: the first leads.
pub const TOGETHER: f64 = 0.012;
/// A trigger-only stroke this soon after its own drum's stroke…
pub const FALSE_DOUBLE_WITHIN: f64 = 0.050;
/// …and this much quieter is the trigger firing twice.
pub const FALSE_DOUBLE_DB: f64 = 6.0;
/// A grace note: this soon before a main stroke of its drum…
pub const GRACE_WITHIN: f64 = 0.050;
/// …and this much quieter.
pub const GRACE_DB: f64 = 6.0;
/// A stroke goes to its own drum's written note this near (after the
/// lean).
pub const NOTE_WINDOW: f64 = 0.080;
/// Two takes' strokes this near at a seam are one stroke heard twice.
pub const SEAM_WITHIN: f64 = 0.020;
/// A move longer than this is worth a look.
pub const LONG_MOVE: f64 = 0.047;
/// The two best lines this close (cost, seconds) is a tie worth a look.
pub const TIE: f64 = 0.010;
/// The lean is never taken past this.
const MAX_LEAN: f64 = 0.120;
/// How much nearer a written note's line counts than a bare line.
const NOTE_PULL_MIDI: f64 = 0.060;
const NOTE_PULL_GRID: f64 = 0.008;
/// What breaking a straight run (not one line apart) costs.
const RUN_BREAK: f64 = 0.030;

/// The song's beats: a hit's time as a beat count from bar 1's start
/// and back, through the bars.
struct Beats<'a> {
    bars: &'a [BarLine],
    /// Each bar's first beat, counted from the first bar's start.
    first: Vec<f64>,
}

impl<'a> Beats<'a> {
    fn new(bars: &'a [BarLine]) -> Self {
        let mut first = Vec::with_capacity(bars.len());
        let mut n = 0.0;
        for b in bars {
            first.push(n);
            n += f64::from(b.beats.max(1));
        }
        Self { bars, first }
    }

    /// The bar `t` is in (the first or last when outside them).
    fn bar_index(&self, t: f64) -> usize {
        self.bars
            .partition_point(|b| b.at <= t)
            .saturating_sub(1)
            .min(self.bars.len().saturating_sub(1))
    }

    fn beat_of(&self, t: f64) -> f64 {
        let i = self.bar_index(t);
        let (b, f) = (&self.bars[i], self.first[i]);
        f + (t - b.at) / b.beat
    }

    fn time_of(&self, beat: f64) -> f64 {
        let i = self
            .first
            .partition_point(|f| *f <= beat)
            .saturating_sub(1)
            .min(self.bars.len().saturating_sub(1));
        let (b, f) = (&self.bars[i], self.first[i]);
        b.at + (beat - f) * b.beat
    }

    /// The bar a beat count is in and the beat within it (0-based).
    fn in_bar(&self, beat: f64) -> (usize, f64) {
        let i = self
            .first
            .partition_point(|f| *f <= beat)
            .saturating_sub(1)
            .min(self.bars.len().saturating_sub(1));
        (i, beat - self.first[i])
    }

    /// One whole note's `division` as a step in beats of bar `i`.
    fn step(&self, i: usize, division: u32) -> f64 {
        f64::from(self.bars[i].unit.max(1)) / f64::from(division.max(1))
    }
}

/// Which drum leads a unison: the snare, then the kick, then the toms.
fn rank(piece: &str) -> u8 {
    let p = piece.to_ascii_lowercase();
    if p.contains("snare") {
        0
    } else if p.contains("kick") {
        1
    } else if p.contains("tom") {
        2
    } else {
        3
    }
}

fn is_tom(piece: &str) -> bool {
    piece.to_ascii_lowercase().contains("tom")
}

fn is_snare(piece: &str) -> bool {
    piece.to_ascii_lowercase().contains("snare")
}

/// A leader's line: where, why, how sure, the line nearest it as played
/// (after the lean), and the other line worth a look, with why.
#[derive(Clone, Debug)]
struct Fit {
    line: f64,
    reason: Reason,
    confidence: f64,
    near: f64,
    doubt: Option<(f64, String)>,
}

/// The line of a whole note's `division` nearest `t`.
fn nearest_line(beats: &Beats<'_>, t: f64, division: u32) -> f64 {
    let b = beats.beat_of(t);
    let (bi, _) = beats.in_bar(b);
    let step = beats.step(bi, division);
    beats.time_of((b / step).round() * step)
}

/// What became of a hit before its line is chosen.
#[derive(Clone, Debug)]
enum Fate {
    Lead,
    Rides(usize, Reason),
    Removed,
    Kept,
    /// Leads, on a fill's own grid.
    FillGrid(u32),
}

/// Plans `request`.
#[must_use]
pub fn plan(request: &PlanRequest) -> Plan {
    let _span = tracing::info_span!(
        "rhythm.plan",
        hits = request.hits.len(),
        notes = request.notes.len(),
        bars = request.bars.len()
    )
    .entered();
    if request.bars.is_empty() || request.hits.is_empty() {
        return Plan::default();
    }
    let mut hits: Vec<&PlayedHit> = request.hits.iter().collect();
    hits.sort_by(|a, b| a.time.total_cmp(&b.time));
    let beats = Beats::new(&request.bars);

    let mut fate: Vec<Fate> = vec![Fate::Lead; hits.len()];
    {
        let _s = tracing::info_span!("rhythm.plan.doubles").entered();
        false_doubles(&hits, &mut fate);
        seam_doubles(&hits, &mut fate);
        let removed = fate.iter().filter(|f| matches!(f, Fate::Removed)).count();
        let seams = fate
            .iter()
            .filter(|f| matches!(f, Fate::Rides(_, Reason::SeamDouble)))
            .count();
        tracing::info!(removed, seams, "doubles");
    }
    {
        let _s = tracing::info_span!("rhythm.plan.zones").entered();
        for (i, h) in hits.iter().enumerate() {
            if !matches!(fate[i], Fate::Lead) {
                continue;
            }
            if let Some(z) = request
                .zones
                .iter()
                .find(|z| h.time >= z.start - 0.010 && h.time < z.end)
            {
                fate[i] = match (z.kind, z.grid) {
                    (ZoneKind::Fill, Some(g)) => Fate::FillGrid(g),
                    (ZoneKind::Fill, None) if request.settings.plan_fills => Fate::Lead,
                    _ => Fate::Kept,
                };
            }
        }
    }
    let notes = notes_by_piece(request);
    {
        let _s = tracing::info_span!("rhythm.plan.rides").entered();
        rides(&hits, &mut fate, &notes);
        let n = fate.iter().filter(|f| matches!(f, Fate::Rides(..))).count();
        tracing::info!(rides = n, "rides");
    }
    let leans = {
        let _s = tracing::info_span!("rhythm.plan.lean").entered();
        let l = leans(request, &hits, &fate, &beats, &notes);
        tracing::info!(halves = l.len(), "lean");
        l
    };
    // Between half bars' middles, the lean goes from one to the next: a
    // stroke at a bar line leans as the bar it lands in does.
    let middles: Vec<(f64, f64)> = leans
        .iter()
        .map(|((i, half), l)| {
            let quarter = f64::from(request.bars[*i].beats) / 4.0;
            (
                beats.first[*i] + quarter * (1.0 + 2.0 * f64::from(*half)),
                l.lean,
            )
        })
        .collect();
    let lean_at = |t: f64| -> f64 {
        let b = beats.beat_of(t);
        let k = middles.partition_point(|(m, _)| *m <= b);
        match (
            k.checked_sub(1).and_then(|k| middles.get(k)),
            middles.get(k),
        ) {
            (Some(&(m0, l0)), Some(&(m1, l1))) if m1 > m0 => l0 + (l1 - l0) * (b - m0) / (m1 - m0),
            (Some(&(_, l)), _) | (None, Some(&(_, l))) => l,
            (None, None) => 0.0,
        }
    };
    let triplet = {
        let _s = tracing::info_span!("rhythm.plan.grid").entered();
        let t = triplet_beats(request, &hits, &fate, &beats, &notes, &lean_at);
        tracing::info!(triplet_beats = t.len(), "grid");
        t
    };

    let mut target: Vec<Option<Fit>> = vec![None; hits.len()];
    {
        let _s = tracing::info_span!("rhythm.plan.lines").entered();
        let mut by_piece: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (i, h) in hits.iter().enumerate() {
            if matches!(fate[i], Fate::Lead | Fate::FillGrid(_)) {
                by_piece.entry(h.piece.as_str()).or_default().push(i);
            }
        }
        let mut solved = 0usize;
        for (piece, idx) in by_piece {
            let piece_notes = notes.get(piece).map_or(&[][..], Vec::as_slice);
            let fit = fit_lines(
                request,
                &hits,
                &idx,
                &fate,
                &beats,
                piece_notes,
                &triplet,
                &lean_at,
            );
            for (k, i) in idx.iter().enumerate() {
                target[*i] = fit[k].clone();
            }
            solved += idx.len();
        }
        tracing::info!(leaders = solved, "lines");
    }

    let _s = tracing::info_span!("rhythm.plan.answer").entered();
    let mut out = Plan::default();
    for (i, h) in hits.iter().enumerate() {
        let (t, reason, confidence) = match &fate[i] {
            Fate::Removed => {
                out.remove.push(h.id.clone());
                (h.time, Reason::FalseDouble, 0.9)
            }
            Fate::Kept => (h.time, Reason::Kept, 1.0),
            Fate::Rides(lead, why) => {
                let shift = lead_shift(*lead, &hits, &fate, &target);
                let gap = (hits[*lead].time - h.time).abs();
                if matches!(why, Reason::Rides(_))
                    && h.piece != hits[*lead].piece
                    && (0.010..0.020).contains(&gap)
                {
                    let own = nearest_line(&beats, h.time - lean_at(h.time), 16);
                    if (own - (h.time + shift)).abs() > 0.0005 {
                        out.doubts.push(Doubt {
                            id: h.id.clone(),
                            piece: h.piece.clone(),
                            time: h.time,
                            candidates: [h.time + shift, own],
                            why: format!(
                                "rides the {} {:.0} ms away, or leads",
                                hits[*lead].piece.to_lowercase(),
                                gap * 1000.0
                            ),
                        });
                    }
                }
                let mut first = *lead;
                while let Fate::Rides(next, _) = &fate[first] {
                    first = *next;
                }
                out.rides.insert(h.id.clone(), hits[first].id.clone());
                (h.time + shift, why.clone(), 0.9)
            }
            Fate::Lead | Fate::FillGrid(_) => match &target[i] {
                Some(fit) => {
                    out.targets.insert(h.id.clone(), fit.line);
                    let moved = (fit.line - h.time).abs();
                    let doubt = fit.doubt.clone().or_else(|| {
                        (moved > LONG_MOVE)
                            .then(|| (fit.near, format!("moves {:.0} ms", moved * 1000.0)))
                    });
                    if let Some((other, why)) = doubt.filter(|(o, _)| (o - fit.line).abs() > 0.0005)
                    {
                        out.doubts.push(Doubt {
                            id: h.id.clone(),
                            piece: h.piece.clone(),
                            time: h.time,
                            candidates: [fit.line, other],
                            why,
                        });
                    }
                    (fit.line, fit.reason.clone(), fit.confidence)
                }
                None => (h.time, Reason::Kept, 0.5),
            },
        };
        out.hits.push(Planned {
            id: h.id.clone(),
            target: t,
            reason,
            confidence,
        });
    }
    // A fill planned like the groove: one question for the whole fill,
    // on its first stroke — or left as played?
    if request.settings.plan_fills {
        for z in request
            .zones
            .iter()
            .filter(|z| z.kind == ZoneKind::Fill && z.grid.is_none())
        {
            let first = hits.iter().zip(&out.hits).find(|(h, p)| {
                h.time >= z.start - 0.010 && h.time < z.end && (p.target - h.time).abs() > 0.0005
            });
            if let Some((h, p)) = first {
                out.doubts.push(Doubt {
                    id: h.id.clone(),
                    piece: h.piece.clone(),
                    time: h.time,
                    candidates: [p.target, h.time],
                    why: format!(
                        "a fill ({:.2}–{:.2} s) planned on the grid, or left as played",
                        z.start, z.end
                    ),
                });
            }
        }
    }
    out.doubts.sort_by(|a, b| a.time.total_cmp(&b.time));
    out.leans = {
        let mut l: Vec<Lean> = leans.into_values().collect();
        l.sort_by_key(|a| (a.bar, a.half));
        l
    };
    tracing::info!(
        targets = out.targets.len(),
        remove = out.remove.len(),
        delete = out.delete.len(),
        rides = out.rides.len(),
        doubts = out.doubts.len(),
        "answer"
    );
    out
}

/// How far a rider's leader moves (following a chain of rides).
fn lead_shift(mut lead: usize, hits: &[&PlayedHit], fate: &[Fate], target: &[Option<Fit>]) -> f64 {
    for _ in 0..8 {
        match &fate[lead] {
            Fate::Rides(next, _) => lead = *next,
            _ => break,
        }
    }
    target[lead]
        .as_ref()
        .map_or(0.0, |f| f.line - hits[lead].time)
}

/// The written notes per piece, by time.
fn notes_by_piece(request: &PlanRequest) -> HashMap<&str, Vec<f64>> {
    let mut out: HashMap<&str, Vec<f64>> = HashMap::new();
    for n in &request.notes {
        out.entry(n.piece.as_str()).or_default().push(n.time);
    }
    for v in out.values_mut() {
        v.sort_by(f64::total_cmp);
        v.dedup_by(|a, b| (*a - *b).abs() < 0.002);
    }
    out
}

fn near_note(notes: &[f64], t: f64, window: f64) -> Option<f64> {
    let j = notes.partition_point(|n| *n < t - window);
    notes[j..]
        .iter()
        .take_while(|n| **n <= t + window)
        .min_by(|a, b| (*a - t).abs().total_cmp(&(*b - t).abs()))
        .copied()
}

/// A trigger-only stroke just after a louder stroke of its own drum, and
/// a tom heard only on its mic beside a louder tom, unsure: one stroke
/// read twice.
fn false_doubles(hits: &[&PlayedHit], fate: &mut [Fate]) {
    for (i, h) in hits.iter().enumerate() {
        if h.flagged("false-double") {
            fate[i] = Fate::Removed;
            continue;
        }
        let before = hits[..i]
            .iter()
            .rev()
            .take_while(|o| h.time - o.time <= FALSE_DOUBLE_WITHIN);
        let mut double = false;
        for o in before {
            if o.piece == h.piece
                && h.heard == Heard::Trigger
                && o.peak_db - h.peak_db >= FALSE_DOUBLE_DB
            {
                double = true;
            }
        }
        if !double && is_tom(&h.piece) && h.flagged("unsure") && h.heard == Heard::Mic {
            double = hits.iter().any(|o| {
                o.piece != h.piece
                    && is_tom(&o.piece)
                    && (o.time - h.time).abs() <= 0.015
                    && o.peak_db - h.peak_db >= FALSE_DOUBLE_DB
            });
        }
        if double {
            fate[i] = Fate::Removed;
        }
    }
}

/// Two takes' strokes of one drum this near, across a take switch: one
/// stroke heard twice in the crossfade. The take that plays on rides the
/// first, so both move as one and neither doubles.
fn seam_doubles(hits: &[&PlayedHit], fate: &mut [Fate]) {
    for i in 0..hits.len() {
        for j in i + 1..hits.len() {
            let (a, b) = (hits[i], hits[j]);
            if b.time - a.time > SEAM_WITHIN {
                break;
            }
            if a.piece != b.piece || a.take == b.take || a.take.is_empty() || b.take.is_empty() {
                continue;
            }
            if !matches!(fate[i], Fate::Lead) || !matches!(fate[j], Fate::Lead) {
                continue;
            }
            fate[j] = Fate::Rides(i, Reason::SeamDouble);
        }
    }
}

/// Grace notes ride their main stroke; strokes within [`UNISON`] are one
/// gesture led by the snare, then the kick, then the toms (a drum with a
/// written note there before one without; for one drum, the first).
fn rides(hits: &[&PlayedHit], fate: &mut [Fate], notes: &HashMap<&str, Vec<f64>>) {
    let leads = |f: &Fate| matches!(f, Fate::Lead | Fate::FillGrid(_) | Fate::Kept);
    // Grace notes first: quieter, just before a main stroke of their drum.
    for i in 0..hits.len() {
        if !leads(&fate[i]) {
            continue;
        }
        let h = hits[i];
        let main = (i + 1..hits.len())
            .take_while(|&j| hits[j].time - h.time <= GRACE_WITHIN)
            .find(|&j| {
                let m = hits[j];
                m.piece == h.piece
                    && leads(&fate[j])
                    && m.time - h.time > UNISON
                    && (m.peak_db - h.peak_db >= GRACE_DB || h.flagged("flam"))
            });
        if let Some(j) = main {
            fate[i] = Fate::Rides(j, Reason::FlamGrace(hits[j].id.clone()));
        }
    }
    // A kick just before a snare (within [`KICK_UNDER`]) is under it:
    // it rides the snare, just before.
    for i in 0..hits.len() {
        if !leads(&fate[i]) || rank(&hits[i].piece) != 1 {
            continue;
        }
        let snare = (i + 1..hits.len())
            .take_while(|&j| hits[j].time - hits[i].time <= KICK_UNDER)
            .find(|&j| {
                is_snare(&hits[j].piece) && leads(&fate[j]) && hits[j].time - hits[i].time >= UNISON
            });
        if let Some(j) = snare {
            fate[i] = Fate::Rides(j, Reason::Rides(hits[j].id.clone()));
        }
    }
    // Unisons: a group from its first stroke.
    let mut i = 0;
    while i < hits.len() {
        if !leads(&fate[i]) {
            i += 1;
            continue;
        }
        let group: Vec<usize> = (i..hits.len())
            .take_while(|&j| hits[j].time - hits[i].time < UNISON)
            .filter(|&j| leads(&fate[j]))
            .collect();
        if group.len() > 1 {
            let written = |j: usize| {
                notes
                    .get(hits[j].piece.as_str())
                    .and_then(|n| near_note(n, hits[j].time, NOTE_WINDOW))
                    .is_some()
            };
            // Two strokes struck together (within [`TOGETHER`]): the
            // first leads. Else the snare, then the kick, then the toms.
            let lead = if hits[group[1]].time - hits[group[0]].time < TOGETHER
                && hits[group[0]].piece != hits[group[1]].piece
                && rank(&hits[group[0]].piece) < 2
            {
                group[0]
            } else {
                *group
                    .iter()
                    .min_by_key(|&&j| (rank(&hits[j].piece), u8::from(!written(j)), j))
                    .unwrap_or(&i)
            };
            // A zone's rule holds for the gesture: kept if its lead is.
            for &j in &group {
                if j != lead {
                    fate[j] = Fate::Rides(lead, Reason::Rides(hits[lead].id.clone()));
                }
            }
        }
        i = group.last().map_or(i + 1, |l| l + 1).max(i + 1);
    }
}

/// The lean per half bar (by bar index): a Viterbi path over half bars
/// through the leans from -[`MAX_LEAN`] to +[`MAX_LEAN`], each half bar's
/// cost how far its strokes, so corrected, are from where they belong
/// (the backbeat snare from its beat, a stroke from its own drum's
/// written note, any stroke from a 16th, weakest), each change of lean
/// from one half to the next a cost too. A dense 16th part alone would
/// fit as well a 16th either way; its neighbours and the backbeat say
/// which.
fn leans(
    request: &PlanRequest,
    hits: &[&PlayedHit],
    fate: &[Fate],
    beats: &Beats<'_>,
    notes: &HashMap<&str, Vec<f64>>,
) -> BTreeMap<(usize, u8), Lean> {
    const STEP: f64 = 0.002;
    const CAP: f64 = 0.035;
    const W_BACKBEAT: f64 = 3.0;
    const W_NOTE: f64 = 1.0;
    const W_GRID: f64 = 0.25;
    const CHANGE: f64 = 1.0;
    let states: Vec<f64> = {
        let n = (MAX_LEAN / STEP).round() as i32;
        (-n..=n).map(|k| f64::from(k) * STEP).collect()
    };
    let halves = request.bars.len() * 2;
    let half_of = |t: f64| {
        let (i, within) = beats.in_bar(beats.beat_of(t));
        i * 2 + usize::from(within >= f64::from(request.bars[i].beats) / 2.0)
    };
    let half_time = half_time_sections(request, hits, fate, beats);
    // Each half's evidence: (weight, the places the stroke may belong,
    // its time).
    let mut evidence: Vec<Vec<(f64, f64, Vec<f64>)>> = vec![Vec::new(); halves];
    let mut measured = vec![0u32; halves];
    for (i, h) in hits.iter().enumerate() {
        if !matches!(fate[i], Fate::Lead | Fate::FillGrid(_)) {
            continue;
        }
        let k = half_of(h.time);
        if k >= halves {
            continue;
        }
        let b = beats.beat_of(h.time);
        let (_, within) = beats.in_bar(b);
        let mut placed = false;
        for a in anchors_at(request, &half_time, &h.piece, h.time) {
            let a0 = a - 1.0;
            if (within - a0).abs() <= 0.35 {
                evidence[k].push((W_BACKBEAT, h.time, vec![beats.time_of(b - within + a0)]));
                placed = true;
            }
        }
        if !placed && let Some(ns) = notes.get(h.piece.as_str()) {
            let j = ns.partition_point(|n| *n < h.time - MAX_LEAN - CAP);
            let near: Vec<f64> = ns[j..]
                .iter()
                .take_while(|n| **n <= h.time + MAX_LEAN + CAP)
                .map(|n| snap_note(beats, *n))
                .collect();
            if !near.is_empty() {
                evidence[k].push((W_NOTE, h.time, near));
                placed = true;
            }
        }
        if !placed {
            let bi = beats.bar_index(h.time);
            let step = beats.step(bi, 16);
            let lines: Vec<f64> = (-2..=2)
                .map(|d| beats.time_of(((b / step).round() + f64::from(d)) * step))
                .collect();
            evidence[k].push((W_GRID, h.time, lines));
        } else {
            measured[k] += 1;
        }
    }
    let emit = |k: usize, lean: f64| -> f64 {
        evidence[k]
            .iter()
            .map(|(w, t, places)| {
                let c = t - lean;
                w * places.iter().map(|p| (c - p).abs()).fold(CAP, f64::min)
            })
            .sum()
    };
    // Viterbi, with a light pull to no lean where nothing says.
    let ns = states.len();
    let mut cost: Vec<f64> = states.iter().map(|l| emit(0, *l) + 0.1 * l.abs()).collect();
    let mut back: Vec<Vec<u16>> = Vec::with_capacity(halves);
    back.push(vec![0; ns]);
    for k in 1..halves {
        let mut next = vec![f64::INFINITY; ns];
        let mut from = vec![0u16; ns];
        // The best predecessor for each state: a distance transform of
        // `cost` under CHANGE·|Δ| (two sweeps).
        let mut best = cost.clone();
        let mut arg: Vec<u16> = (0..ns as u16).collect();
        for s in 1..ns {
            let via = best[s - 1] + CHANGE * STEP;
            if via < best[s] {
                best[s] = via;
                arg[s] = arg[s - 1];
            }
        }
        for s in (0..ns - 1).rev() {
            let via = best[s + 1] + CHANGE * STEP;
            if via < best[s] {
                best[s] = via;
                arg[s] = arg[s + 1];
            }
        }
        for s in 0..ns {
            next[s] = best[s] + emit(k, states[s]) + 0.1 * states[s].abs();
            from[s] = arg[s];
        }
        cost = next;
        back.push(from);
    }
    let mut s = (0..ns)
        .min_by(|a, b| cost[*a].total_cmp(&cost[*b]))
        .unwrap_or(ns / 2);
    let mut path = vec![0usize; halves];
    for k in (0..halves).rev() {
        path[k] = s;
        s = usize::from(back[k][s]);
    }
    (0..halves)
        .map(|k| {
            let (bi, half) = (k / 2, (k % 2) as u8);
            (
                (bi, half),
                Lean {
                    bar: request.bars[bi].bar,
                    half,
                    lean: states[path[k]],
                    anchors: measured[k],
                },
            )
        })
        .collect()
}

/// The sections whose snare is on 3 more than on 2 and 4.
fn half_time_sections(
    request: &PlanRequest,
    hits: &[&PlayedHit],
    fate: &[Fate],
    beats: &Beats<'_>,
) -> Vec<bool> {
    request
        .sections
        .iter()
        .map(|s| {
            let (mut on3, mut on24) = (0, 0);
            for (i, h) in hits.iter().enumerate() {
                if !matches!(fate[i], Fate::Lead)
                    || !is_snare(&h.piece)
                    || h.time < s.from
                    || h.time >= s.to
                {
                    continue;
                }
                let (bi, within) = beats.in_bar(beats.beat_of(h.time));
                if request.bars[bi].beats != 4 {
                    continue;
                }
                if (within - 2.0).abs() < 0.2 {
                    on3 += 1;
                } else if (within - 1.0).abs() < 0.2 || (within - 3.0).abs() < 0.2 {
                    on24 += 1;
                }
            }
            on3 > on24
        })
        .collect()
}

/// The anchor beats (1-based) for `piece` at `t`: the settings' for its
/// section, else the snare's backbeat (3 in half time, else 2 and 4).
fn anchors_at(request: &PlanRequest, half_time: &[bool], piece: &str, t: f64) -> Vec<f64> {
    let section = request
        .sections
        .iter()
        .position(|s| t >= s.from && t < s.to);
    let name = section.map(|i| request.sections[i].name.as_str());
    let set: Vec<&daw_edit::plan::Anchor> = request
        .settings
        .anchors
        .iter()
        .filter(|a| a.section.is_none() || a.section.as_deref() == name)
        .collect();
    if !set.is_empty() {
        return set
            .into_iter()
            .filter(|a| a.piece.eq_ignore_ascii_case(piece))
            .flat_map(|a| a.beats.iter().copied())
            .collect();
    }
    if !is_snare(piece) {
        return Vec::new();
    }
    if section.is_some_and(|i| half_time.get(i).copied().unwrap_or(false)) {
        vec![3.0]
    } else {
        vec![2.0, 4.0]
    }
}

/// A written note's time on its line: the nearest 16th, 8th-triplet or
/// 16th-triplet line within 4 ms, else the note as written.
fn snap_note(beats: &Beats<'_>, n: f64) -> f64 {
    let b = beats.beat_of(n);
    let (bi, _) = beats.in_bar(b);
    let mut best = n;
    let mut gap = 0.004;
    for div in [16, 12, 24, 32] {
        let step = beats.step(bi, div);
        let line = beats.time_of((b / step).round() * step);
        if (line - n).abs() < gap {
            gap = (line - n).abs();
            best = line;
        }
    }
    best
}

/// The beats (as beat counts) to put on triplets, and which: 12 (8th
/// triplets) or 24 (16th triplets).
fn triplet_beats(
    request: &PlanRequest,
    hits: &[&PlayedHit],
    fate: &[Fate],
    beats: &Beats<'_>,
    notes: &HashMap<&str, Vec<f64>>,
    lean_at: &dyn Fn(f64) -> f64,
) -> BTreeMap<i64, u32> {
    let mut out = BTreeMap::new();
    let off16 = |x: f64| ((x * 4.0) - (x * 4.0).round()).abs() > 0.08;
    // The written notes on a triplet line and off a 16th.
    let mut written: BTreeMap<i64, (u32, u32)> = BTreeMap::new();
    let mut straight: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
    for ns in notes.values() {
        for &n in ns {
            let b = beats.beat_of(n);
            let frac = b - b.floor();
            let t8 = ((frac * 3.0) - (frac * 3.0).round()).abs() < 0.06;
            let t16 = ((frac * 6.0) - (frac * 6.0).round()).abs() < 0.06;
            let on_beat = frac < 0.06 || frac > 0.94;
            if off16(frac) {
                let e = written.entry(b.floor() as i64).or_default();
                if t8 {
                    e.0 += 1;
                } else if t16 {
                    e.1 += 1;
                }
            } else if !on_beat && !t16 {
                // On a 16th and on no triplet line: a straight beat.
                straight.insert(b.floor() as i64);
            }
        }
    }
    // The strokes' spacing.
    let mut played: BTreeMap<i64, Vec<f64>> = BTreeMap::new();
    for (i, h) in hits.iter().enumerate() {
        if !matches!(fate[i], Fate::Lead) {
            continue;
        }
        let b = beats.beat_of(h.time - lean_at(h.time)) + 1.0 / 12.0;
        played
            .entry(b.floor() as i64)
            .or_default()
            .push(b - 1.0 / 12.0 - b.floor());
    }
    let hinted = |beat: i64| {
        let t = beats.time_of(beat as f64 + 0.5);
        request
            .sections
            .iter()
            .find(|s| t >= s.from && t < s.to)
            .and_then(|s| request.settings.grid_hint.get(&s.name).copied())
    };
    let keys: Vec<i64> = written.keys().chain(played.keys()).copied().collect();
    for beat in keys {
        if out.contains_key(&beat) {
            continue;
        }
        if let Some(g) = hinted(beat) {
            if g % 3 == 0 {
                out.insert(beat, g);
            }
            continue;
        }
        let (w8, w16) = written.get(&beat).copied().unwrap_or((0, 0));
        // The bar's triplet notes: one alone is the part written loosely,
        // two or more a triplet figure.
        let (bi, _) = beats.in_bar(beat as f64);
        let bar_first = beats.first[bi] as i64;
        let in_bar: u32 = (bar_first..bar_first + i64::from(request.bars[bi].beats))
            .filter_map(|b| written.get(&b))
            .map(|(a, b)| a + b)
            .sum();
        if !straight.contains(&beat) && (w8 + w16 >= 2 || (w8 + w16 >= 1 && in_bar >= 2)) {
            out.insert(beat, if w16 > 0 { 24 } else { 12 });
            continue;
        }
        if straight.contains(&beat) {
            continue;
        }
        let Some(p) = played.get(&beat) else { continue };
        if p.len() < 3 {
            continue;
        }
        let err = |div: f64| {
            p.iter()
                .map(|x| ((x * div) - (x * div).round()).abs() / div)
                .sum::<f64>()
                / p.len() as f64
        };
        let (e16, e8t, e16t) = (err(4.0), err(3.0), err(6.0));
        // In beats: 0.03 beat is ~11 ms at 160.
        if e16 > 0.05 && e16t < e16 * 0.45 {
            out.insert(beat, if e8t < 0.03 { 12 } else { 24 });
        }
    }
    out
}

/// The lines a stroke may take near `c` (corrected time): the beat's grid
/// (16ths, its triplets, or a fill's own), each with whether it is a
/// written note's.
fn candidates(
    beats: &Beats<'_>,
    c: f64,
    grid: Option<u32>,
    triplet: &BTreeMap<i64, u32>,
    notes: &[f64],
    prefer: Prefer,
) -> Vec<(f64, f64, bool)> {
    let b = beats.beat_of(c);
    let (bi, _) = beats.in_bar(b);
    let mut out: Vec<(f64, f64, bool)> = Vec::new();
    let push = |line: f64, out: &mut Vec<(f64, f64, bool)>| {
        if !out.iter().any(|(l, ..)| (l - line).abs() < 0.0005) {
            out.push((line, (line - c).abs(), false));
        }
    };
    let divs: Vec<u32> = match grid {
        Some(g) => vec![g],
        None => {
            let lo = (b - 0.6).floor() as i64;
            let hi = (b + 0.6).floor() as i64;
            let mut d = vec![];
            for beat in lo..=hi {
                d.push(triplet.get(&beat).copied().unwrap_or(16));
            }
            d.sort_unstable();
            d.dedup();
            d
        }
    };
    for div in divs {
        let step = beats.step(bi, div);
        let k = (b / step).round();
        for dk in -2..=2 {
            let lb = (k + f64::from(dk)) * step;
            // A triplet line only where its beat is a triplet beat (or
            // on the beat itself), a 16th only off them.
            let beat = lb.floor() as i64;
            let on_beat = (lb - lb.round()).abs() < 1e-6;
            let beat_div = grid.unwrap_or_else(|| triplet.get(&beat).copied().unwrap_or(16));
            if !on_beat && beat_div != div {
                continue;
            }
            let line = beats.time_of(lb);
            if (line - c).abs() <= 0.16 {
                push(line, &mut out);
            }
            // Past half a line from the stroke, every bit counts twice:
            // a stroke goes to a farther line only when it must.
            let half = step * beats.bars[bi].beat / 2.0;
            if let Some(e) = out
                .last_mut()
                .filter(|e| (e.0 - line).abs() < 0.0005 && !e.2)
            {
                e.1 += 2.0 * (e.1 - half).max(0.0);
            }
        }
    }
    let pull = match prefer {
        Prefer::Midi => NOTE_PULL_MIDI,
        Prefer::Grid => NOTE_PULL_GRID,
    };
    let j = notes.partition_point(|n| *n < c - NOTE_WINDOW);
    for &n in notes[j..].iter().take_while(|n| **n <= c + NOTE_WINDOW) {
        // On its beat's grid: a triplet note in a straight beat is the
        // part written loosely, not a triplet played.
        let nb = beats.beat_of(n);
        let div = grid.unwrap_or_else(|| triplet.get(&(nb.floor() as i64)).copied().unwrap_or(16));
        let (nbi, _) = beats.in_bar(nb);
        let step = beats.step(nbi, div);
        let line = beats.time_of((nb / step).round() * step);
        if let Some(e) = out.iter_mut().find(|(l, ..)| (l - line).abs() < 0.0005) {
            // A written note's line costs its distance alone, less the
            // pull: no penalty for being far, within the window.
            e.1 = (line - c).abs() - pull;
            e.2 = true;
        } else {
            out.push((line, (line - c).abs() - pull, true));
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// Every leader of one drum on a line, in order, one stroke per line:
/// each a target, a reason, a confidence and the next best line.
#[allow(clippy::too_many_arguments)]
fn fit_lines(
    request: &PlanRequest,
    hits: &[&PlayedHit],
    idx: &[usize],
    fate: &[Fate],
    beats: &Beats<'_>,
    notes: &[f64],
    triplet: &BTreeMap<i64, u32>,
    lean_at: &dyn Fn(f64) -> f64,
) -> Vec<Option<Fit>> {
    let n = idx.len();
    if n == 0 {
        return Vec::new();
    }
    let corrected: Vec<f64> = idx
        .iter()
        .map(|&i| hits[i].time - lean_at(hits[i].time))
        .collect();
    let cands: Vec<Vec<(f64, f64, bool)>> = idx
        .iter()
        .zip(&corrected)
        .map(|(&i, &c)| {
            let grid = match fate[i] {
                Fate::FillGrid(g) => Some(g),
                _ => None,
            };
            candidates(beats, c, grid, triplet, notes, request.settings.prefer)
        })
        .collect();
    // A straight run: strokes about a 16th apart.
    let sixteenth = |t: f64| {
        let bi = beats.bar_index(t);
        beats.step(bi, 16) * request.bars[bi].beat
    };
    let in_run: Vec<bool> = (0..n)
        .map(|k| {
            k > 0 && {
                let gap = hits[idx[k]].time - hits[idx[k - 1]].time;
                let s = sixteenth(hits[idx[k]].time);
                gap > 0.7 * s && gap < 1.3 * s
            }
        })
        .collect();
    let run_len = {
        // How many strokes the run each stroke is in has.
        let mut len = vec![1usize; n];
        let mut start = 0;
        for k in 1..=n {
            if k == n || !in_run[k] {
                for l in &mut len[start..k] {
                    *l = k - start;
                }
                start = k;
            }
        }
        len
    };
    // dp[k][c]: the least cost of strokes 0..=k with stroke k on cands[k][c].
    let mut cost: Vec<Vec<f64>> = Vec::with_capacity(n);
    let mut back: Vec<Vec<usize>> = Vec::with_capacity(n);
    for k in 0..n {
        let mut row = vec![f64::INFINITY; cands[k].len()];
        let mut from = vec![usize::MAX; cands[k].len()];
        for (c, (line, own, _)) in cands[k].iter().enumerate() {
            if k == 0 {
                row[c] = *own;
                continue;
            }
            for (p, (pline, ..)) in cands[k - 1].iter().enumerate() {
                if cost[k - 1][p].is_infinite() || *pline >= line - 0.0005 {
                    continue;
                }
                let mut step = cost[k - 1][p] + own;
                if request.settings.lock_runs && in_run[k] && run_len[k] >= 3 {
                    let s = sixteenth(*line);
                    if ((line - pline) - s).abs() > 0.002 {
                        step += RUN_BREAK;
                    }
                }
                if step < row[c] {
                    row[c] = step;
                    from[c] = p;
                }
            }
        }
        if row.iter().all(|c| c.is_infinite()) {
            // No line after the last one's: take its own nearest, alone.
            for (c, (_, own, _)) in cands[k].iter().enumerate() {
                row[c] = cost[k - 1].iter().copied().fold(f64::INFINITY, f64::min) + own + 1.0;
                from[c] = usize::MAX;
            }
        }
        cost.push(row);
        back.push(from);
    }
    let mut choice = vec![usize::MAX; n];
    let last = &cost[n - 1];
    let mut c = (0..last.len())
        .min_by(|a, b| last[*a].total_cmp(&last[*b]))
        .unwrap_or(usize::MAX);
    for k in (0..n).rev() {
        choice[k] = c;
        if c == usize::MAX {
            break;
        }
        let p = back[k][c];
        if k > 0 && p == usize::MAX {
            // A broken chain: the stroke before chooses alone.
            let prev = &cost[k - 1];
            c = (0..prev.len())
                .min_by(|a, b| prev[*a].total_cmp(&prev[*b]))
                .unwrap_or(usize::MAX);
        } else {
            c = p;
        }
    }
    (0..n)
        .map(|k| {
            let c = choice[k];
            let (line, own, written) = *cands[k].get(c)?;
            // The next best line for this stroke, its neighbours held.
            let lo = if k > 0 && choice[k - 1] != usize::MAX { cands[k - 1][choice[k - 1]].0 } else { f64::NEG_INFINITY };
            let hi = if k + 1 < n && choice[k + 1] != usize::MAX { cands[k + 1][choice[k + 1]].0 } else { f64::INFINITY };
            let other = cands[k]
                .iter()
                .enumerate()
                .filter(|(o, (l, ..))| *o != c && *l > lo + 0.0005 && *l < hi - 0.0005)
                .min_by(|a, b| a.1 .1.total_cmp(&b.1 .1))
                .map(|(_, e)| *e);
            let margin = other.map_or(0.05, |o| o.1 - own);
            let confidence = (margin / 0.03).clamp(0.0, 1.0);
            let reason = match fate[idx[k]] {
                Fate::FillGrid(g) => Reason::FillGrid(g),
                _ if written => Reason::Midi,
                _ if in_run[k] && run_len[k] >= 3 || k + 1 < n && in_run[k + 1] && run_len[k] >= 3 => Reason::DoubleRun,
                _ => Reason::Grid,
            };
            let c = corrected[k];
            let near = nearest_line(beats, c, 16);
            // The other grid's nearest line: a 16th for a triplet line, a
            // triplet line for a 16th.
            let on16 = (nearest_line(beats, line, 16) - line).abs() < 0.0005;
            let cross = if on16 {
                [nearest_line(beats, c, 12), nearest_line(beats, c, 24)]
                    .into_iter()
                    .filter(|l| (nearest_line(beats, *l, 16) - l).abs() > 0.0005)
                    .min_by(|a, b| (a - c).abs().total_cmp(&(b - c).abs()))
            } else {
                Some(near)
            };
            let doubt = if margin < TIE {
                other.map(|o| (o.0, "two lines nearly as near".to_owned()))
            } else if let Some(x) = cross
                .filter(|_| !(on16 && written))
                .filter(|x| (x - c).abs() < (line - c).abs() + if on16 { -0.004 } else { 0.012 })
            {
                Some((x, if on16 { "straight or triplet" } else { "triplet or straight" }.to_owned()))
            } else if written && (near - c).abs() + 0.025 < (line - c).abs() {
                Some((near, "the written note, or the line it was played on".to_owned()))
            } else {
                None
            };
            let confidence = if doubt.is_some() { confidence.min(0.3) } else { confidence };
            tracing::debug!(
                hit.id = %hits[idx[k]].id,
                hit.time = hits[idx[k]].time,
                plan.corrected = c,
                plan.line = line,
                plan.reason = %reason,
                plan.candidates = ?cands[k].iter().map(|(l, cost, w)| (l, cost, w)).collect::<Vec<_>>(),
                "plan.line"
            );
            Some(Fit { line, reason, confidence, near, doubt })
        })
        .collect()
}

#[cfg(test)]
mod tests;
