//! The timing core: which line a played event goes to, which events
//! move together, and how a run is kept in order.
//!
//! Lifted from Session's drum quantize (`QUANTIZE`) without a daw: every
//! function here takes times in seconds and a `line_of` to find a grid
//! line, so the engine around it (reading hit markers, the tempo map,
//! writing the slip) is the caller's. Spec: `docs/spec/session/drum-editing.md`
//! (`r[quantize.*]`); design: `docs/design/editing.md`.

use facet::Facet;

/// One hit's move.
#[derive(Facet)]
pub struct SlipMove {
    pub time: f64,
    pub bar: i32,
    pub beat: f64,
    /// Where its target is, ms from it (late positive: the hit is late
    /// when this is negative).
    pub off_ms: f64,
    /// How far it is moved, ms.
    pub shift_ms: f64,
}

/// What a slip edit did, or would.
#[derive(Facet)]
pub struct SlipResult {
    pub hits: u32,
    pub moved: u32,
    /// Within tolerance already.
    pub in_place: u32,
    /// No target near: left as played.
    pub unmatched: u32,
    /// In a fill, and left as played.
    pub in_fills: u32,
    /// How far the moved hits were, ms: median and worst.
    pub median_off_ms: Option<f64>,
    pub worst_off_ms: Option<f64>,
    pub applied: bool,
    /// The biggest moves, first.
    pub biggest: Vec<SlipMove>,
    /// Left as played, with no target near enough to be sure of: to be
    /// listened to (the first 50).
    pub to_check: Vec<SlipMove>,
}

/// The nearest of `written` (sorted) within `window` of `h`.
pub fn nearest(written: &[f64], h: f64, window: f64) -> Option<f64> {
    let at = written.partition_point(|n| *n < h - window);
    written
        .iter()
        .skip(at)
        .take_while(|n| **n <= h + window)
        .min_by(|x, y| (*x - h).abs().total_cmp(&(*y - h).abs()))
        .copied()
}

/// The line a stroke with no note of its own goes to: of the grid asked
/// for (`division`, a line of it from `line_of`) and each coarser one down
/// to the sixteenths (the eighth triplets on a triplet grid), the one it
/// sits nearest for that grid's reach — `sure` on the grid asked for,
/// twice it a grid coarser, and so on; a tie to the coarser line. `None`
/// when it is in reach of none.
///
/// On a thirty-second grid the second stroke of a double kick played 70 ms
/// after the first sat 23 ms from a thirty-second and 24 ms from its
/// sixteenth — past `sure` (15 ms) of either, and left as played, the
/// roll ragged (Unbreakable, bar 63). In reach of the sixteenth it is the
/// sixteenth's; a stroke played tight on a thirty-second stays on it.
pub fn grid_choice(
    line_of: impl Fn(u32) -> f64,
    felt: f64,
    division: u32,
    sure: f64,
) -> Option<f64> {
    let floor = if division.is_multiple_of(3) { 12 } else { 16 };
    let mut best: Option<(f64, f64)> = None;
    let mut level = division.max(1);
    loop {
        let line = line_of(level);
        let reach = sure * f64::from(division.max(1)) / f64::from(level.max(1));
        let fit = (line - felt).abs() / reach.max(f64::EPSILON);
        // Coarser levels come later: a tie goes to them.
        if fit <= 1.0 && best.is_none_or(|(was, _)| fit <= was + 1e-9) {
            best = Some((fit, line));
        }
        let coarser = level / 2;
        if !level.is_multiple_of(2) || coarser < floor {
            break;
        }
        level = coarser;
    }
    best.map(|(_, line)| line)
}

/// Whether a stroke played at `felt` keeps its own line rather than going
/// to `line` (a written one): played on its own line (`on_a_line`), past
/// `sure` of the written one; in reach of it, past half a sixteenth
/// (`step`) — and then only a line at the sixteenth or coarser (`own` is
/// `(its line, that line's nearest sixteenth)`).
///
/// In reach of a thirty-second is no line of the drummer's: on a 32nd
/// grid nearly every stroke is within reach of one, and the 89.1 kick
/// Unbreakable's drummer pushed 68 ms ahead sat 15 ms from 88.4.875 —
/// kept there against its own note on 89.1, a downbeat a 32nd early.
pub fn keeps_own_line(
    (felt, line): (f64, f64),
    own: Option<(f64, f64)>,
    on_a_line: bool,
    (sure, step): (f64, f64),
) -> bool {
    let Some((own, coarse)) = own else {
        return false;
    };
    let off = (line - felt).abs();
    if on_a_line {
        return off > sure;
    }
    (own - coarse).abs() < 1e-4 && off > sure && off > step / 2.0
}

/// How near a grid line a stroke must be played to be on it, seconds: its
/// line is the drummer's, past the chart's say (see `targets`). On
/// Unbreakable the chart moved kicks played dead on their sixteenths a
/// whole sixteenth (bar 105) and onto triplets (bar 63), and kicks played
/// near theirs 60–75 ms onto the chart's eighths (bars 100, 136, 164).
pub const PLAYED_ON_A_LINE: f64 = 0.005;

/// How far a written note may be from a thirty-second or a sixteenth
/// triplet and still be on it, seconds.
pub const ON_ITS_LINE: f64 = 0.006;

/// The share of a fill's hits the drum MIDI must account for to be
/// edited to it: a fill written in the chart — the same tom figure every
/// pre-chorus — is part of the song, not the drummer's own.
pub const WRITTEN_FILL: f64 = 0.8;

/// How far past a sixteenth from where it was played a hit may be sent,
/// seconds: a drummer a full sixteenth and more off a written line is
/// playing another line.
pub const FURTHEST_PAST: f64 = 0.010;

/// How near a line the chart writes something on a hit with no note of
/// its own piece must be, its feel taken out, to be that line's —
/// sixteenths.
pub const NEAR_WRITTEN: f64 = 0.4;

/// What placed a moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placed {
    /// Its piece's note in the drum MIDI ([`crate::written::align`]).
    Written,
    /// No note of its own: a line the chart writes something on, near —
    /// a stroke the drummer added among written ones.
    Near,
    /// The grid: the nearest line, the section's feel taken out.
    Line,
    /// Nothing near enough to be sure of.
    Nothing,
}

/// Where strokes are sent to one line (`placed`, in time order), the one
/// placed surest (`by`) — else the one nearest it — keeps it, and the
/// others take the grid's lines (`step_of(i)` apart) either side in order:
/// those before it the lines before, those after the lines after. A stroke
/// already on the line it would take is pushed on in turn. Only as far as
/// a stroke may go — a step and a quarter from where it was played — and
/// never past the stroke beyond; else it is left to [`one_stroke_a_line`].
///
/// Unbreakable 73.1: kicks played at 1.776 and 1.884, the snare on 2. The
/// second kick was sent to 2 with the snare and moved between — 32 ms off
/// any line; the hand put the two on the "&" and the "a".
pub fn to_lines_either_side(
    placed: &mut [(f64, Option<f64>)],
    by: &[Placed],
    (step_of, on_a_beat, with_next): (
        impl Fn(usize) -> Option<f64>,
        impl Fn(f64) -> bool,
        impl Fn(usize, usize) -> bool,
    ),
) {
    let rank = |p: Option<&Placed>| match p {
        Some(Placed::Written) => 0_u8,
        Some(Placed::Near) => 1,
        Some(Placed::Line) => 2,
        Some(Placed::Nothing) | None => 3,
    };
    let same = |a: f64, b: f64| (a - b).abs() < 0.002;
    let mut i = 0_usize;
    while i < placed.len() {
        let Some(&(_, Some(line))) = placed.get(i) else {
            i = i.saturating_add(1);
            continue;
        };
        let mut end = i.saturating_add(1);
        while placed
            .get(end)
            .is_some_and(|(_, to)| to.is_some_and(|t| same(t, line)))
        {
            end = end.saturating_add(1);
        }
        // Strokes of different drums played a gesture apart are one
        // gesture on one line — a kick 32 ms before the snare on 2 — not
        // two to spread: spread, the kick pushed the double before it a
        // line back (Unbreakable 71.1).
        let one_gesture = (i..end.saturating_sub(1)).all(|k| with_next(k, k.saturating_add(1)));
        if end.saturating_sub(i) < 2 || one_gesture {
            i = end;
            continue;
        }
        let keeper = (i..end).min_by(|x, y| {
            let off = |k: usize| {
                placed
                    .get(k)
                    .map_or(f64::INFINITY, |(h, _)| (h - line).abs())
            };
            rank(by.get(*x))
                .cmp(&rank(by.get(*y)))
                .then(off(*x).total_cmp(&off(*y)))
        });
        let (Some(keeper), Some(step)) = (keeper, step_of(i)) else {
            i = end;
            continue;
        };
        let reach = step * 1.25;
        let fits = |k: usize, to: f64| -> bool {
            placed.get(k).is_some_and(|(h, _)| (to - h).abs() <= reach)
        };
        // With `keeper` keeping the line: those before it back a line each,
        // pushing back what is on the line they take; those after it on a
        // line each, short of the next moment's line. `None`: does not fit.
        let plan = |keeper: usize| -> Option<Vec<(usize, f64)>> {
            // A stroke played on a beat keeps it: never pushed a step off
            // for one played beside it (a kick 1.5 ms before the one, a
            // tom's grace 35 ms before that: the kick went a sixteenth
            // late, Unbreakable 77.1 and 83.1). Off the beat it can go —
            // a double's first stroke near the "a" belongs on the "&".
            let on_it = |k: usize| {
                on_a_beat(line)
                    && placed
                        .get(k)
                        .is_some_and(|(h, _)| (h - line).abs() < step * ON_THE_LINE)
            };
            if (i..end).any(|k| k != keeper && on_it(k)) {
                return None;
            }
            let mut moves: Vec<(usize, f64)> = Vec::new();
            let mut want = line;
            let mut k = keeper;
            while let Some(prev) = k.checked_sub(1) {
                want -= step;
                let &(_, to) = placed.get(prev)?;
                let in_group = prev >= i;
                let on_it = to.is_some_and(|t| t > want - 0.002);
                if !in_group && !on_it {
                    break;
                }
                if !fits(prev, want) {
                    return None;
                }
                moves.push((prev, want));
                k = prev;
            }
            let mut want = line;
            let mut k = keeper;
            while k.saturating_add(1) < end {
                want += step;
                k = k.saturating_add(1);
                if !fits(k, want) {
                    return None;
                }
                moves.push((k, want));
            }
            // Short of the next moment's line — or on it, played with it: a
            // kick 22 ms before the snare on 2 is the snare's flam, not a
            // stroke to push the double back for (Unbreakable 67.1, 71.1).
            let next = placed.get(end).and_then(|(_, to)| *to);
            next.is_none_or(|next| {
                moves.iter().all(|(k, t)| {
                    *t < next - 0.002 || ((t - next).abs() < 0.002 && with_next(*k, end))
                })
            })
            .then_some(moves)
        };
        // The surest keeps it if the rest fit; else the last (the rest
        // back — a double played into a snare on the next line); else the
        // first (the rest on).
        let last = end.saturating_sub(1);
        if let Some(moves) = plan(keeper).or_else(|| plan(last)).or_else(|| plan(i)) {
            for (k, to) in moves {
                if let Some(slot) = placed.get_mut(k) {
                    slot.1 = Some(to);
                }
            }
        }
        i = end;
    }
}

/// One stroke to a line: where moments a gesture apart are sent to the
/// same line, the one placed by its own note keeps it (else the one
/// nearer it), and each other is moved between its neighbours — by as
/// much as the moved hits either side of it, in proportion to where it
/// sits between them.
///
/// A double kick with a stroke the chart does not have (seventeen hits in
/// a bar of sixteenths) sent that stroke to its neighbour's line, and the
/// two were moved as one gesture by the louder's shift: the extra stroke
/// landed 23 ms after the one before, in its decay — the markers "in the
/// middle of the kick" on Unbreakable's outro. Moved between, the roll
/// stays a roll: evenly spaced where it was evenly played.
pub fn one_stroke_a_line(placed: &mut [(f64, Option<f64>)], by: &[Placed]) {
    let rank = |p: Option<&Placed>| match p {
        Some(Placed::Written) => 0_u8,
        Some(Placed::Near) => 1,
        Some(Placed::Line) => 2,
        Some(Placed::Nothing) | None => 3,
    };
    // Who keeps each shared line, and who is moved between.
    let mut between: Vec<usize> = Vec::new();
    let mut start = 0_usize;
    while start < placed.len() {
        let Some(&(_, Some(line))) = placed.get(start) else {
            start = start.saturating_add(1);
            continue;
        };
        // The moments after it sent to the same line, a gesture apart.
        let mut end = start.saturating_add(1);
        while let (Some(&(prev, _)), Some(&(next, Some(to)))) =
            (placed.get(end.saturating_sub(1)), placed.get(end))
        {
            if (to - line).abs() >= 0.002 || next - prev < GESTURE {
                break;
            }
            end = end.saturating_add(1);
        }
        let keeper = (start..end).min_by(|a, b| {
            let off = |i: usize| {
                placed
                    .get(i)
                    .map_or(f64::INFINITY, |(h, _)| (h - line).abs())
            };
            rank(by.get(*a))
                .cmp(&rank(by.get(*b)))
                .then(off(*a).total_cmp(&off(*b)))
        });
        between.extend((start..end).filter(|i| Some(*i) != keeper));
        start = end;
    }
    for &i in &between {
        if let Some(slot) = placed.get_mut(i) {
            slot.1 = None;
        }
    }
    // Each moved by its neighbours' shifts, interpolated; with a moved
    // neighbour near on one side only, by that one's; with none near, cut
    // where it was played.
    let shifts: Vec<Option<(f64, f64)>> = placed
        .iter()
        .map(|(h, to)| to.map(|t| (*h, t - h)))
        .collect();
    for &i in &between {
        let Some(&(h, _)) = placed.get(i) else {
            continue;
        };
        let near = |(at, _): &(f64, f64)| (at - h).abs() <= NEIGHBOUR_REACH;
        let before = shifts.iter().take(i).rev().find_map(|s| *s).filter(near);
        let after = shifts
            .iter()
            .skip(i.saturating_add(1))
            .find_map(|s| *s)
            .filter(near);
        let shift = match (before, after) {
            (Some((a, sa)), Some((b, sb))) if b > a => {
                let along = ((h - a) / (b - a)).clamp(0.0, 1.0);
                Some((sb - sa).mul_add(along, sa))
            }
            (Some((_, s)), _) | (None, Some((_, s))) => Some(s),
            (None, None) => None,
        };
        if let Some(slot) = placed.get_mut(i) {
            slot.1 = shift.map(|s| h + s);
        }
    }
}

/// How far a moved neighbour may be from a stroke [`one_stroke_a_line`]
/// moves between, seconds: a sixteenth at 60 bpm. Further, it is not a
/// neighbour in the same figure.
pub const NEIGHBOUR_REACH: f64 = 0.25;

/// Moments nearer than this are one gesture, moved as one, seconds:
/// under a 1/32 at 160 bpm (47 ms), so a double kick's 16ths and a
/// drummer's 32nds stay their own.
pub const GESTURE: f64 = 0.040;

/// How far apart two strokes of one drum must be played, and sent, to be a
/// double — two strokes — rather than one stroke and its echo, seconds.
pub const DOUBLE_STROKE: f64 = 0.020;

/// How far apart strokes of two drums must be played to be two strokes —
/// a kick into the snare — rather than one gesture (a flam, a drag),
/// seconds.
pub const TWO_STROKES: f64 = 0.025;

/// How near a beat, as a grid step's fraction, a stroke is played on it.
pub const ON_THE_LINE: f64 = 0.25;

/// What a seam takes before a hit: its lead and its crossfade, seconds.
pub const SEAM_ROOM: f64 = 0.010;

/// Each gesture — moments nearer together than [`GESTURE`] (a kick into
/// a snare, a drag, a flam across pieces), or sent to targets that would
/// close them up past half their spacing (two hits pulled onto one grid
/// line) — moved by one shift, its loudest targeted moment's, so the
/// gesture keeps its own timing; none when no moment of it has a target.
///
/// Moved apart, a slice moved later reaches back into its own source to
/// close the gap, and with the hit before it nearer than that reach, it
/// plays that hit's attack again: a kick 19 ms before a snare, the snare
/// sent 18.8 ms later on its own, was heard twice on Unbreakable.
pub fn hold_gestures(pairs: &[(f64, Option<f64>)], loudness: &[f64]) -> Vec<(f64, Option<f64>)> {
    hold_gestures_by(pairs, loudness, |_| false)
}

/// [`hold_gestures`], with `apart(k)` saying the moments `k - 1` and `k`
/// are two strokes whatever the gesture rules say — a double kick, its two
/// kicks sent to lines of their own.
pub fn hold_gestures_by(
    pairs: &[(f64, Option<f64>)],
    loudness: &[f64],
    apart: impl Fn(usize) -> bool,
) -> Vec<(f64, Option<f64>)> {
    let mut out: Vec<(f64, Option<f64>)> = Vec::with_capacity(pairs.len());
    let mut start = 0_usize;
    while start < pairs.len() {
        let mut end = start.saturating_add(1);
        while let (Some(prev), Some(next)) = (pairs.get(end.saturating_sub(1)), pairs.get(end)) {
            // Sent to targets that close up more than half the room
            // between them — the same grid line, often, from either side —
            // two moments are one gesture as surely as two close ones.
            let collide = match (prev.1, next.1) {
                (Some(a), Some(b)) => b - a < 0.5 * (next.0 - prev.0),
                _ => false,
            };
            // Moved later than the one before by more than the room between
            // them, a slice reaches back over that one's attack to close its
            // gap: two kicks 47 ms apart, the second sent 57 ms later, played
            // the first twice. Together they keep their spacing.
            let shift = |(h, t): &(f64, Option<f64>)| t.map_or(0.0, |t| t - h);
            let reaches = shift(next) - shift(prev) > next.0 - prev.0 - SEAM_ROOM;
            if apart(end) || (next.0 - prev.0 >= GESTURE && !collide && !reaches) {
                break;
            }
            end = end.saturating_add(1);
        }
        let group = pairs.get(start..end).unwrap_or_default();
        let shift = group
            .iter()
            .enumerate()
            .filter_map(|(k, (h, t))| {
                let t = (*t)?;
                Some((
                    loudness
                        .get(start.saturating_add(k))
                        .copied()
                        .unwrap_or(0.0),
                    t - h,
                ))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, shift)| shift);
        out.extend(group.iter().map(|(h, _)| (*h, shift.map(|s| h + s))));
        start = end;
    }
    out
}

/// A rest longer than this many grid steps ends a locked run: the next
/// is placed on its own.
pub const RUN_BREAK: f64 = 2.5;

/// How many of a run's first hits place it.
pub const ANCHOR_HITS: usize = 4;

/// How far from a timekeeping hit (a grid step's fraction) a locked hit
/// can be and still be the one played with it.
pub const ANCHOR_REACH: f64 = 0.6;

/// `hits` (sorted, seconds) locked in order to the lines of a grid `step`
/// seconds apart: each hit the line after the one before it — or as many
/// lines after as the hits are steps apart — so a run of 16ths stays a
/// run of 16ths, never two on one line or one skipped, however far the
/// player drifts from the grid within it. Each run (hits less than
/// [`RUN_BREAK`] steps apart) is placed by its first [`ANCHOR_HITS`],
/// `line_of` giving the line nearest a time. Each hit with its target.
///
/// What "nearest line" gets wrong: a double kick dragging steadily — the
/// hands on the grid, the feet up to 0.24 beat behind by a bar's end on
/// Unbreakable — has its later kicks nearer the *next* 16th than their
/// own, and nearest-line sends them there.
pub fn lock_in_order(hits: &[f64], step: f64, line_of: impl Fn(f64) -> f64) -> Vec<(f64, f64)> {
    let mut out = Vec::with_capacity(hits.len());
    let mut start = 0_usize;
    while start < hits.len() {
        // The run, and each hit's line counted from the run's first.
        let mut idx: Vec<f64> = vec![0.0];
        let mut end = start.saturating_add(1);
        while let (Some(prev), Some(next)) = (hits.get(end.saturating_sub(1)), hits.get(end)) {
            let steps = (next - prev) / step;
            if steps > RUN_BREAK {
                break;
            }
            let last = idx.last().copied().unwrap_or(0.0);
            idx.push(last + steps.round().max(1.0));
            end = end.saturating_add(1);
        }
        let run = hits.get(start..end).unwrap_or_default();
        let first = run.first().copied().unwrap_or(0.0);
        let base = line_of(first);
        // Placed by how it starts: a run begins where it is meant to and
        // drifts from there, so fitting the whole of it would let the
        // drift pull every hit a line along.
        let cost = |a: f64| -> f64 {
            run.iter()
                .zip(&idx)
                .take(ANCHOR_HITS)
                .map(|(h, i)| (h - (i + a).mul_add(step, base)).abs())
                .sum()
        };
        let anchor = [-3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0]
            .into_iter()
            .min_by(|a, b| cost(*a).total_cmp(&cost(*b)))
            .unwrap_or(0.0);
        out.extend(
            run.iter()
                .zip(&idx)
                .map(|(h, i)| (*h, (i + anchor).mul_add(step, base))),
        );
        start = end;
    }
    out
}

/// `hits` (sorted) between two anchored ones, the first on `from` and the
/// last on `to` (both lines), each sent to a line between them in order,
/// one a line: the order that lands them nearest their hits. With more
/// hits than lines, each to its nearest (the gestures they make are held
/// whole after).
pub fn spread_between(hits: &[f64], (from, to): (f64, f64), step: f64) -> Vec<f64> {
    let lines = ((to - from) / step).round();
    let free = num_traits::ToPrimitive::to_usize(&(lines - 1.0).max(0.0)).unwrap_or(0);
    let line = |k: usize| {
        step.mul_add(
            num_traits::ToPrimitive::to_f64(&k.saturating_add(1)).unwrap_or(0.0),
            from,
        )
    };
    if hits.len() > free || free == 0 {
        return hits
            .iter()
            .map(|h| {
                let k = ((h - from) / step).round() - 1.0;
                let k = num_traits::ToPrimitive::to_usize(&k.max(0.0))
                    .unwrap_or(0)
                    .min(free.saturating_sub(1));
                line(k)
            })
            .collect();
    }
    // cost[i][k]: the least cost of the first i+1 hits with hit i on line k.
    let m = hits.len();
    let inf = f64::INFINITY;
    let mut cost = vec![vec![inf; free]; m];
    let mut from_k = vec![vec![0_usize; free]; m];
    for (i, h) in hits.iter().enumerate() {
        let mut best = (inf, 0_usize);
        for k in 0..free {
            if i > 0 && k > 0 {
                let prev = cost
                    .get(i.saturating_sub(1))
                    .and_then(|r| r.get(k.saturating_sub(1)))
                    .copied()
                    .unwrap_or(inf);
                if prev < best.0 {
                    best = (prev, k.saturating_sub(1));
                }
            }
            let before = if i == 0 { 0.0 } else { best.0 };
            if let Some(c) = cost.get_mut(i).and_then(|r| r.get_mut(k)) {
                *c = before + (h - line(k)).abs();
            }
            if let Some(f) = from_k.get_mut(i).and_then(|r| r.get_mut(k)) {
                *f = best.1;
            }
        }
    }
    let mut k = cost
        .get(m.saturating_sub(1))
        .and_then(|r| {
            r.iter()
                .enumerate()
                .min_by(|a, b| a.1.total_cmp(b.1))
                .map(|(k, _)| k)
        })
        .unwrap_or(0);
    let mut out = vec![0.0; m];
    for i in (0..m).rev() {
        if let Some(o) = out.get_mut(i) {
            *o = line(k);
        }
        k = from_k.get(i).and_then(|r| r.get(k)).copied().unwrap_or(0);
    }
    out
}

/// `hits` (sorted) locked to a grid `step` apart with `anchors`: hits
/// known to be on a line (index into `hits`, the line) — a kick played
/// with the snare on 2 and 4, the hands keeping time while the feet
/// drift. Between two anchors the hits take the lines between them in
/// order ([`spread_between`]); before the first and after the last they
/// run on in order ([`lock_in_order`]) from it. With no anchors, the
/// whole of `hits` is locked in order.
///
/// Counted forward from where a run starts alone, a double kick that
/// drags, then squeezes a beat to catch up, lands a whole 16th out by the
/// next bar — the snare on 2 pulled to the 16th before it, on Unbreakable.
pub fn lock_anchored(
    hits: &[f64],
    anchors: &[(usize, f64)],
    step: f64,
    line_of: impl Fn(f64) -> f64 + Copy,
) -> Vec<(f64, f64)> {
    let (Some(&(first, first_line)), Some(&(last, last_line))) = (anchors.first(), anchors.last())
    else {
        return lock_in_order(hits, step, line_of);
    };
    let mut to: Vec<f64> = vec![0.0; hits.len()];
    // Before the first anchor: counted back from it.
    let before = hits.get(..first).unwrap_or_default();
    let mut slot = first_line;
    let mut at = hits.get(first).copied().unwrap_or(0.0);
    for (i, h) in before.iter().enumerate().rev() {
        slot -= step * ((at - h) / step).round().max(1.0);
        at = *h;
        if let Some(t) = to.get_mut(i) {
            *t = slot;
        }
    }
    for pair in anchors.windows(2) {
        let [(a, la), (b, lb)] = pair else { continue };
        if let Some(t) = to.get_mut(*a) {
            *t = *la;
        }
        let between = hits.get(a.saturating_add(1)..*b).unwrap_or_default();
        for (k, line) in spread_between(between, (*la, *lb), step)
            .into_iter()
            .enumerate()
        {
            if let Some(t) = to.get_mut(a.saturating_add(1).saturating_add(k)) {
                *t = line;
            }
        }
    }
    if let Some(t) = to.get_mut(last) {
        *t = last_line;
    }
    // After the last: counted on from it.
    let mut slot = last_line;
    let mut at = hits.get(last).copied().unwrap_or(0.0);
    for (i, h) in hits.iter().enumerate().skip(last.saturating_add(1)) {
        slot += step * ((h - at) / step).round().max(1.0);
        at = *h;
        if let Some(t) = to.get_mut(i) {
            *t = slot;
        }
    }
    hits.iter().copied().zip(to).collect()
}

/// Which of `hits` (sorted) a timekeeping hit at `at` anchors: within
/// [`ANCHOR_REACH`] of a grid step, the one played before it if any —
/// moved onto `at`'s line it never crosses `at` — else the one after.
pub fn anchor_of(hits: &[f64], at: f64, step: f64) -> Option<usize> {
    let reach = ANCHOR_REACH * step;
    let before = hits
        .iter()
        .enumerate()
        .filter(|(_, h)| **h <= at + 0.005 && at - **h <= reach)
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i);
    before.or_else(|| {
        hits.iter()
            .enumerate()
            .filter(|(_, h)| **h > at && **h - at <= reach)
            .min_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i)
    })
}

/// `pairs` with no target sent across a moment that stays: a slice
/// cannot be moved past audio played before it, and one that was played
/// the snare after, sent back over it onto its beat, played the snare
/// twice. A hit that would cross stays, as the one it would cross does.
pub fn uncrossed(mut pairs: Vec<(f64, Option<f64>)>) -> Vec<(f64, Option<f64>)> {
    let stays: Vec<f64> = pairs
        .iter()
        .filter(|(_, t)| t.is_none())
        .map(|(h, _)| *h)
        .collect();
    for (h, t) in &mut pairs {
        if let Some(to) = *t
            && stays
                .iter()
                .any(|s| (*s > *h && to >= *s - 0.002) || (*s < *h && to <= *s + 0.002))
        {
            *t = None;
        }
    }
    pairs
}

/// How far the slice holding `t` moves: the one cut last at or before it
/// (`slices`: each cut and its slice's shift, any order); none before the
/// first.
pub fn shift_at(slices: &[(f64, f64)], t: f64) -> f64 {
    slices
        .iter()
        .filter(|(cut, _)| *cut <= t + 1e-9)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(0.0, |(_, shift)| *shift)
}
