//! The drum part as written, held up to the part as played: which note
//! each hit is.
//!
//! A drummer sits ahead of the click or behind it for a whole section —
//! a pre-chorus pushed 40 ms early, a breakdown laid back — and inside
//! that feel a hit can wander a long way: a kick written on 4, played
//! 100 ms early, is nearer the sixteenth before 4 than 4 itself. The
//! nearest line puts it there, a sixteenth early for good, and every bar
//! of the section the same. The written part knows it is on 4.
//!
//! So each piece's hits are matched to that piece's own notes ([`align`]):
//! first the feel — how far the notes sit from the hits nearby that are
//! plainly theirs ([`feel_at`]) — then each hit, moved by the feel, to the
//! nearest note not already taken, one to one, within a sixteenth. A hit with no
//! note near (a variation, a fill the chart does not have) is matched to
//! nothing and placed some other way. A note with no hit is not played.

/// How near a note a hit must be to say what the feel is, sixteenths.
const PLAIN: f64 = 0.5;
/// How far from a hit, its feel taken out, its note may be, sixteenths:
/// one — a drummer pushing a section 40 ms puts some hits 60 to 100 ms
/// ahead of their notes, past the line before. Taken one to one, in
/// order, a hit the chart does not have takes no note from one it does.
const REACH: f64 = 1.0;
/// How far a hit and a note that are each other's nearest may be,
/// sixteenths: a little over one.
const MUTUAL: f64 = 1.1;

/// Fewer plain matches than this and there is no feel to speak of.
const FEEL_FROM: usize = 4;

/// How many of the nearest plain matches a hit's feel is read from: a
/// drummer drifts — a pre-chorus pushed in its first half, dragged in its
/// second — so the feel is local.
const FEEL_OF: usize = 8;

/// One piece's hits matched to its notes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Aligned {
    /// Each hit's note, in the hits' order.
    pub notes: Vec<Option<f64>>,
}

/// The note nearest `t` in `notes` (by time).
fn nearest(notes: &[f64], t: f64) -> Option<f64> {
    let at = notes.partition_point(|n| *n < t);
    [at.checked_sub(1), Some(at)]
        .into_iter()
        .flatten()
        .filter_map(|i| notes.get(i).copied())
        .min_by(|a, b| (a - t).abs().total_cmp(&(b - t).abs()))
}

/// The hits plainly their notes — within half a sixteenth (`step`
/// seconds) — and how far each note sits after its hit: `(hit, offset)`.
#[must_use]
pub fn plain(hits: &[f64], notes: &[f64], step: f64) -> Vec<(f64, f64)> {
    hits.iter()
        .filter_map(|h| nearest(notes, *h).map(|n| (*h, n - h)))
        .filter(|(_, d)| d.abs() <= PLAIN * step)
        .collect()
}

/// The feel at `t`: how far the notes sit after the hits there.
///
/// Positive when the drummer plays ahead: the median of the [`FEEL_OF`]
/// plain matches (`(hit, offset)`, by time) nearest it; none with fewer
/// than [`FEEL_FROM`].
#[must_use]
pub fn feel_at(plain: &[(f64, f64)], t: f64) -> f64 {
    if plain.len() < FEEL_FROM {
        return 0.0;
    }
    let at = plain.partition_point(|(h, _)| *h < t);
    let (mut lo, mut hi) = (at, at);
    while hi.saturating_sub(lo) < FEEL_OF {
        let left = lo
            .checked_sub(1)
            .and_then(|i| plain.get(i))
            .map(|p| t - p.0);
        let right = plain.get(hi).map(|p| p.0 - t);
        match (left, right) {
            (Some(l), Some(r)) if l <= r => lo = lo.saturating_sub(1),
            (Some(_), None) => lo = lo.saturating_sub(1),
            (_, Some(_)) => hi = hi.saturating_add(1),
            (None, None) => break,
        }
    }
    let mut near: Vec<f64> = plain
        .get(lo..hi)
        .unwrap_or_default()
        .iter()
        .map(|p| p.1)
        .collect();
    near.sort_by(f64::total_cmp);
    near.get(near.len() / 2).copied().unwrap_or(0.0)
}

/// `hits` (one piece's, by time) matched to `notes` (the same piece's
/// written notes, by time), a sixteenth being `step` seconds and the feel
/// at a time `feel` (see [`feel_at`]).
#[must_use]
pub fn align(hits: &[f64], notes: &[f64], step: f64, feel: impl Fn(f64) -> f64) -> Aligned {
    // Every pairing within reach, nearest first; each hit and each note
    // taken once.
    let reach = REACH * step;
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for (i, h) in hits.iter().enumerate() {
        let c = h + feel(*h);
        let from = notes.partition_point(|n| *n < c - reach);
        for (j, n) in notes.iter().enumerate().skip(from) {
            if *n > c + reach {
                break;
            }
            pairs.push(((n - c).abs(), i, j));
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut of_hit: Vec<Option<usize>> = vec![None; hits.len()];
    let mut taken = vec![false; notes.len()];
    for (_, i, j) in pairs {
        if of_hit.get(i).copied().flatten().is_some() || taken.get(j).copied().unwrap_or(true) {
            continue;
        }
        // Never out of order: a hit after another is never given a note
        // before that one's.
        let crosses = of_hit
            .iter()
            .enumerate()
            .any(|(k, other)| other.is_some_and(|o| (k < i && o > j) || (k > i && o < j)));
        if crosses {
            continue;
        }
        if let Some(slot) = of_hit.get_mut(i) {
            *slot = Some(j);
        }
        if let Some(t) = taken.get_mut(j) {
            *t = true;
        }
    }
    // A hit the feel left out of reach — played against the drift around
    // it — is still its note's when they are each other's nearest: no
    // other hit of the piece nearer that note, no other note nearer the
    // hit, within a sixteenth and a little.
    for (i, h) in hits.iter().enumerate() {
        if of_hit.get(i).copied().flatten().is_some() {
            continue;
        }
        let Some((j, n)) = notes
            .iter()
            .enumerate()
            .min_by(|a, b| (a.1 - h).abs().total_cmp(&(b.1 - h).abs()))
        else {
            continue;
        };
        let theirs = hits
            .iter()
            .min_by(|a, b| (*a - n).abs().total_cmp(&(*b - n).abs()))
            .is_some_and(|nearest| (nearest - h).abs() < 1e-12);
        let free = !taken.get(j).copied().unwrap_or(true);
        let crosses = of_hit
            .iter()
            .enumerate()
            .any(|(k, other)| other.is_some_and(|o| (k < i && o > j) || (k > i && o < j)));
        if theirs && free && !crosses && (n - h).abs() <= MUTUAL * step {
            if let Some(slot) = of_hit.get_mut(i) {
                *slot = Some(j);
            }
            if let Some(t) = taken.get_mut(j) {
                *t = true;
            }
        }
    }
    Aligned {
        notes: of_hit
            .into_iter()
            .map(|j| j.and_then(|j| notes.get(j).copied()))
            .collect(),
    }
}
