//! A shut folder's span mixed again where its children changed, not over
//! its whole length: [`remix`].
//!
//! A kit whose cuts are joined is one span the length of the song per
//! lane, its envelope the loudest of 25 000 slices at every point
//! ([`crate::midi::fold_waves`]). An edit moves three of them, and mixing
//! the whole lane again for it was most of what the edit cost the window.
//! The envelope at a point is the loudest of the children's points over
//! it, so where no child changed, it is as it was: this mixes again only
//! the points a changed child reaches, before and after, from every child
//! over them — the same maximum over the same points, so the same numbers.

use std::sync::Arc;

use crate::midi::{Child, Wave};

/// The most points a folded waveform holds — `midi::fold_waves`' own.
const FOLDED_POINTS: usize = 1 << 16;

/// How near a folded point's edge a child's point that meets it is taken
/// to only meet it — `midi::fold_waves`' own.
const EDGE: f64 = 1e-9;

/// One child as a span was mixed from it: where it was and the waveform it
/// had, and what it was (its guid and layer, hashed).
#[derive(Clone)]
pub(super) struct MixChild {
    pub(super) id: u64,
    pub(super) at: f64,
    pub(super) length: f64,
    pub(super) wave: Option<Arc<Wave>>,
}

/// The children of a span, as a later mix compares them.
pub(super) type MixedFrom = Arc<[MixChild]>;

/// Where `now` differs from `was`: the reach of each child that came,
/// went, moved or was read again — as it was and as it is. In any order:
/// a mix is the loudest of its children, whichever comes first.
pub(super) fn changed(was: &[MixChild], now: &[MixChild]) -> Vec<(f64, f64)> {
    let before: rustc_hash::FxHashMap<u64, &MixChild> = was.iter().map(|c| (c.id, c)).collect();
    let mut reach = Vec::new();
    let mut kept = 0_usize;
    for child in now {
        match before.get(&child.id) {
            Some(was) if same(was, child) => kept += 1,
            Some(was) => {
                kept += 1;
                reach.extend(reach_of(was));
                reach.extend(reach_of(child));
            }
            None => reach.extend(reach_of(child)),
        }
    }
    if kept < was.len() {
        let after: rustc_hash::FxHashSet<u64> = now.iter().map(|c| c.id).collect();
        reach.extend(
            was.iter()
                .filter(|c| !after.contains(&c.id))
                .filter_map(reach_of),
        );
    }
    reach
}

/// Whether a child is where it was, with the waveform it had.
fn same(a: &MixChild, b: &MixChild) -> bool {
    let same_wave = match (&a.wave, &b.wave) {
        (Some(x), Some(y)) => Arc::ptr_eq(x, y),
        (None, None) => true,
        _ => false,
    };
    same_wave && a.at.to_bits() == b.at.to_bits() && a.length.to_bits() == b.length.to_bits()
}

/// How far a child's waveform reaches, seconds: every point it has.
fn reach_of(child: &MixChild) -> Option<(f64, f64)> {
    let wave = child.wave.as_ref()?;
    let points = crate::num::coord(wave.points.len());
    Some((child.at, points.mul_add(wave.step, child.at)))
}

/// The folded points' grid `fold_waves` lays `children` on over
/// `start..end`: how many, and how far apart.
fn grid(start: f64, end: f64, children: &[Child<'_>]) -> Option<(usize, f64)> {
    let finest = children
        .iter()
        .filter_map(|c| c.wave.filter(|w| w.step > 0.0 && !w.points.is_empty()))
        .map(|w| w.step)
        .fold(f64::INFINITY, f64::min);
    let span = end - start;
    if !finest.is_finite() || span <= 0.0 {
        return None;
    }
    let count = crate::num::index((span / finest).ceil()).clamp(1, FOLDED_POINTS);
    Some((count, span / crate::num::coord(count)))
}

/// The folded points a stretch `begins..ends` of a child lands in, as
/// `fold_waves` puts it.
fn landing(begins: f64, ends: f64, (start, step, count): (f64, f64, usize)) -> (usize, usize) {
    let last = count.saturating_sub(1);
    let first = crate::num::index(((begins - start) / step + EDGE).floor()).min(last);
    let past = crate::num::index(((ends - start) / step - EDGE).ceil())
        .clamp(first.saturating_add(1), count);
    (first, past)
}

/// `children` mixed over `start..end` as [`crate::midi::fold_waves`] mixes
/// them, from `was` (that mix before the children in `reach` changed):
/// only the points they reach mixed again. A mix on another grid is
/// mixed whole.
pub(super) fn remix(
    was: &Wave,
    (start, end): (f64, f64),
    children: &[Child<'_>],
    reach: &[(f64, f64)],
) -> Option<Wave> {
    let (count, step) = grid(start, end, children)?;
    if was.points.len() != count || was.step.to_bits() != step.to_bits() {
        return crate::midi::fold_waves(start, end, children);
    }
    let at = (start, step, count);
    // The points any change reaches, a point wider each side: those
    // mixed again, from every child over them.
    let mut again: Vec<(usize, usize)> = reach
        .iter()
        .filter(|(a, b)| *b > start && *a < end)
        .map(|&(a, b)| {
            let (first, past) = landing(a, b, at);
            (first.saturating_sub(1), past.saturating_add(1).min(count))
        })
        .collect();
    again.sort_unstable();
    let mut points = was.points.clone();
    for &(first, past) in &again {
        for point in points.iter_mut().take(past).skip(first) {
            *point = (0.0, 0.0);
        }
    }
    for child in children {
        let Some(wave) = child.wave.filter(|w| w.step > 0.0 && !w.points.is_empty()) else {
            continue;
        };
        let ends = crate::num::coord(wave.points.len()).mul_add(wave.step, child.at);
        let (lo, hi) = landing(child.at, ends, at);
        if !again.iter().any(|&(first, past)| lo < past && first < hi) {
            continue;
        }
        for (j, (hi, lo)) in wave.points.iter().enumerate() {
            let begins = crate::num::coord(j).mul_add(wave.step, child.at);
            let ends = begins + wave.step;
            if ends <= start || begins >= end {
                continue;
            }
            let (first, past) = landing(begins, ends, at);
            for &(from, to) in &again {
                let (a, b) = (first.max(from), past.min(to));
                for point in points.iter_mut().take(b).skip(a) {
                    *point = (point.0.max(*hi), point.1.min(*lo));
                }
            }
        }
    }
    Some(Wave { step, points })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave(seed: u32, n: usize, step: f64) -> Wave {
        let mut s = u64::from(seed) | 1;
        let points = (0..n)
            .map(|_| {
                s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                #[expect(clippy::cast_precision_loss, reason = "a made-up peak")]
                let p = (s >> 40) as f32 / (1_u64 << 24) as f32;
                (p, -p * 0.8)
            })
            .collect();
        Wave { step, points }
    }

    /// A take cut in `n` slices of `width` seconds, each its own peaks.
    fn slices(n: usize, width: f64) -> Vec<(f64, f64, Wave)> {
        (0..n)
            .map(|k| {
                let at = crate::num::coord(k) * width;
                let seed = u32::try_from(k).unwrap_or(0);
                (at, width, wave(seed, 19, 0.01))
            })
            .collect()
    }

    fn children(of: &[(f64, f64, Wave)]) -> Vec<Child<'_>> {
        of.iter()
            .map(|(at, length, wave)| Child {
                at: *at,
                length: *length,
                wave: Some(wave),
                notes: None,
            })
            .collect()
    }

    fn mixed(of: &[(f64, f64, Arc<Wave>)]) -> Vec<MixChild> {
        of.iter()
            .enumerate()
            .map(|(k, (at, length, wave))| MixChild {
                id: u64::try_from(k).unwrap_or(0),
                at: *at,
                length: *length,
                wave: Some(Arc::clone(wave)),
            })
            .collect()
    }

    #[test]
    fn a_remix_is_the_mix_made_again() {
        let mut take = slices(600, 0.183);
        let span = (0.0, 600.0 * 0.183);
        let was = crate::midi::fold_waves(span.0, span.1, &children(&take)).expect("a mix");
        let before: Vec<(f64, f64, Arc<Wave>)> = take
            .iter()
            .map(|(a, l, w)| (*a, *l, Arc::new(w.clone())))
            .collect();
        // Three hits moved later, one read again, one moved earlier.
        take[100].0 += 0.005;
        take[101].0 += 0.005;
        take[102].2 = wave(9_999, 19, 0.01);
        take[400].0 -= 0.031;
        let after: Vec<(f64, f64, Arc<Wave>)> = take
            .iter()
            .enumerate()
            .map(|(k, (a, l, w))| {
                let kept = before
                    .get(k)
                    .filter(|b| *b.2 == *w)
                    .map(|b| Arc::clone(&b.2));
                (*a, *l, kept.unwrap_or_else(|| Arc::new(w.clone())))
            })
            .collect();
        // In another order: a mix is the loudest of its children, whichever first.
        let mut now = mixed(&after);
        now.reverse();
        let reach = changed(&mixed(&before), &now);
        assert_eq!(
            reach.len(),
            8,
            "each changed child's reach, before and after"
        );
        let remixed = remix(&was, span, &children(&take), &reach).expect("a mix");
        let fresh = crate::midi::fold_waves(span.0, span.1, &children(&take)).expect("a mix");
        assert_eq!(remixed, fresh);
    }

    #[test]
    fn children_that_come_or_go_are_changes_where_they_reach() {
        let take = slices(4, 0.5);
        let arcs: Vec<(f64, f64, Arc<Wave>)> = take
            .iter()
            .map(|(a, l, w)| (*a, *l, Arc::new(w.clone())))
            .collect();
        let was = mixed(&arcs);
        assert_eq!(changed(&was, &was[..3]).len(), 1, "one went: its reach");
        let mut renamed = was.clone();
        renamed[2].id = 99;
        assert_eq!(changed(&was, &renamed).len(), 2, "one went, another came");
        assert_eq!(changed(&was, &was), Vec::new(), "nothing changed");
    }
}
