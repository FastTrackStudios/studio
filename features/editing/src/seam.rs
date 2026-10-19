//! Where two takes of a kit meet: the cut heard least.
//!
//! A comp switches every mic of the kit from one take to another at once.
//! What gives the switch away is what changes there that nothing covers:
//! the two takes' waveforms disagreeing across the crossfade, and the
//! wash under them (cymbals, rooms, a ringing tom) jumping in level. A
//! loud attack right after the cut covers both; a ghost note covers
//! little; the middle of a decay, nothing. So each candidate cut — just
//! before an attack of either take — is scored, every mic alike, by
//! what changes there over what follows it, and the quietest wins.
//!
//! Nothing here reads a project: each mic's two takes come as audio over
//! one window of the timeline, and times are seconds from its start.

/// One mic of the kit over the window: what plays there now and what the
/// comp would switch to, from the window's start at one rate.
#[derive(Clone, Copy, Debug)]
pub struct Mic<'a> {
    pub out: &'a [f32],
    pub incoming: &'a [f32],
}

/// A cut and how much of it is heard (0: nothing changes; about 1: the
/// change is as loud as what covers it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seam {
    /// Seconds from the window's start.
    pub at: f64,
    pub heard: f64,
}

/// Either side of a cut, how long the takes are compared over, seconds:
/// the crossfade a comp meets with, and a little.
const ACROSS: f64 = 0.006;
/// Before a cut, how much of the wash under it is compared, seconds.
const WASH: f64 = 0.05;
/// After a cut, how long the incoming take covers it, seconds: the
/// attack and the start of its ring.
const COVER: f64 = 0.03;
/// Below this a mic is silent, as a level (-60 dBFS): nothing to hear,
/// nothing to cover.
const FLOOR: f64 = 0.001;
/// How much it costs to move a cut its whole reach from where it was
/// asked for, in `heard`: between two about as quiet, the nearer.
const DRIFT: f64 = 0.15;

/// How much a cut at `at` (seconds from the window's start) is heard.
#[must_use]
pub fn heard_at(mics: &[Mic<'_>], rate: f64, at: f64) -> f64 {
    let frames = |s: f64| num_traits::ToPrimitive::to_usize(&(s * rate).round()).unwrap_or(0);
    let Some(c) = num_traits::ToPrimitive::to_usize(&(at * rate).round()) else {
        return f64::INFINITY;
    };
    let (across, wash, cover) = (frames(ACROSS), frames(WASH), frames(COVER));
    let each: Vec<f64> = mics
        .iter()
        .filter_map(|m| {
            let around = c.checked_sub(across)?..c.checked_add(across)?;
            let before = c.checked_sub(wash)?..c;
            let after = c..c.checked_add(cover)?;
            let differ = rms_of_difference(m.out.get(around.clone())?, m.incoming.get(around)?);
            let jump = (rms(m.out.get(before.clone())?) - rms(m.incoming.get(before)?)).abs();
            let covered = rms(m.incoming.get(after)?).max(FLOOR);
            Some((differ + jump) / covered)
        })
        .collect();
    if each.is_empty() {
        return f64::INFINITY;
    }
    // The kit is heard as a whole, and its worst mic gives a cut away.
    let mean =
        each.iter().sum::<f64>() / num_traits::ToPrimitive::to_f64(&each.len()).unwrap_or(1.0);
    let worst = each.iter().fold(0.0_f64, |w, h| w.max(*h));
    0.5f64.mul_add(worst, mean)
}

/// Of `candidates` (seconds from the window's start), the cut heard
/// least, nearer `wanted` between two alike; `reach` is how far from it a
/// candidate may be. `None` with no candidate the window holds.
#[must_use]
pub fn best(
    mics: &[Mic<'_>],
    rate: f64,
    candidates: &[f64],
    (wanted, reach): (f64, f64),
) -> Option<Seam> {
    candidates
        .iter()
        .filter(|c| (**c - wanted).abs() <= reach)
        .map(|&at| Seam {
            at,
            heard: heard_at(mics, rate, at),
        })
        .filter(|s| s.heard.is_finite())
        .min_by(|a, b| {
            let cost = |s: &Seam| DRIFT.mul_add((s.at - wanted).abs() / reach.max(1e-9), s.heard);
            cost(a).total_cmp(&cost(b))
        })
}

fn rms(x: &[f32]) -> f64 {
    if x.is_empty() {
        return 0.0;
    }
    let sum: f64 = x.iter().map(|s| f64::from(*s).powi(2)).sum();
    (sum / num_traits::ToPrimitive::to_f64(&x.len()).unwrap_or(1.0)).sqrt()
}

fn rms_of_difference(a: &[f32], b: &[f32]) -> f64 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let sum: f64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| (f64::from(*x) - f64::from(*y)).powi(2))
        .sum();
    (sum / num_traits::ToPrimitive::to_f64(&n).unwrap_or(1.0)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;

    /// A take: a ringing wash of `wash` level (its own noise, `seed`), and
    /// hits at `hits` (seconds, level).
    fn take(seconds: f64, wash: f32, seed: u32, hits: &[(f64, f32)]) -> Vec<f32> {
        let n = num_traits::ToPrimitive::to_usize(&(seconds * RATE)).unwrap_or(0);
        let mut state = seed;
        let mut out: Vec<f32> = (0..n)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let r =
                    num_traits::ToPrimitive::to_f32(&(state >> 8)).unwrap_or(0.0) / 16_777_216.0;
                (r - 0.5) * 2.0 * wash
            })
            .collect();
        for &(at, level) in hits {
            let from = num_traits::ToPrimitive::to_usize(&(at * RATE)).unwrap_or(0);
            for (i, s) in out.iter_mut().skip(from).take(4_800).enumerate() {
                let t = num_traits::ToPrimitive::to_f32(&i).unwrap_or(0.0) / 48_000.0;
                *s += level * (std::f32::consts::TAU * 180.0 * t).sin() * (-t * 40.0).exp();
            }
        }
        out
    }

    /// Two takes with different wash under them, a ghost note at 0.3 s and
    /// a backbeat at 0.6 s in both: the cut goes before the backbeat, which
    /// covers it, not before the ghost.
    #[test]
    fn the_cut_goes_where_a_loud_hit_covers_it() {
        let hits = [(0.3, 0.05), (0.6, 0.9)];
        let out = take(1.0, 0.03, 1, &hits);
        let incoming = take(1.0, 0.03, 2, &hits);
        let mics = [Mic {
            out: &out,
            incoming: &incoming,
        }];
        let lead = 0.002;
        let cut = best(&mics, RATE, &[0.3 - lead, 0.6 - lead], (0.3, 0.5)).expect("a cut");
        assert!((cut.at - (0.6 - lead)).abs() < 1e-9, "{cut:?}");
        assert!(heard_at(&mics, RATE, 0.3 - lead) > 2.0 * cut.heard);
    }

    /// Where the wash jumps in level, the cut is heard more than where the
    /// two takes ring alike.
    #[test]
    fn a_jump_in_the_wash_is_heard() {
        let hits = [(0.5, 0.5)];
        let quiet = take(1.0, 0.01, 1, &hits);
        let loud = take(1.0, 0.1, 2, &hits);
        let alike = take(1.0, 0.01, 3, &hits);
        let jump = heard_at(
            &[Mic {
                out: &loud,
                incoming: &quiet,
            }],
            RATE,
            0.498,
        );
        let smooth = heard_at(
            &[Mic {
                out: &alike,
                incoming: &quiet,
            }],
            RATE,
            0.498,
        );
        assert!(jump > 2.0 * smooth, "jump {jump}, smooth {smooth}");
    }

    /// The same take on both sides: nothing changes, nothing is heard.
    #[test]
    fn the_same_take_is_not_heard() {
        let one = take(1.0, 0.05, 1, &[(0.5, 0.5)]);
        let h = heard_at(
            &[Mic {
                out: &one,
                incoming: &one,
            }],
            RATE,
            0.498,
        );
        assert!(h < 1e-9, "{h}");
    }

    /// Between two cuts heard alike, the one nearer where it was asked for.
    #[test]
    fn between_alike_the_nearer() {
        let hits = [(0.3, 0.5), (0.7, 0.5)];
        let one = take(1.0, 0.0, 1, &hits);
        let mics = [Mic {
            out: &one,
            incoming: &one,
        }];
        let cut = best(&mics, RATE, &[0.298, 0.698], (0.65, 0.5)).expect("a cut");
        assert!((cut.at - 0.698).abs() < 1e-9, "{cut:?}");
    }
}
