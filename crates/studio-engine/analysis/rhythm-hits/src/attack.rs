//! Where an attack starts, and how loud it gets.
//!
//! A gate fires once the attack is under way — a millisecond or two in,
//! later for a soft hit — and a trigger's spike crosses its threshold
//! part way up. Neither is where the sound *starts*, which is what a cut
//! has to stay in front of and what Quantize puts on the grid. So the
//! attack is found from its peak: the loudest sample just after the
//! detection, then back along a fine envelope to where it was still
//! under a tenth of that peak.

/// How far before the detection the attack's peak may be, seconds.
const BEFORE: f64 = 0.006;
/// How far after it, seconds. A struck sound peaks within a few ms.
const AFTER: f64 = 0.012;
/// The fine envelope's window, seconds.
const FINE: f64 = 0.0005;
/// How far back from the peak the start is looked for, seconds.
const RISE: f64 = 0.015;
/// The fraction of the peak the attack has risen past at its start.
const START: f32 = 0.1;
/// How long after a hit its peak is read, seconds.
const MEASURE: f64 = 0.010;

/// The loudest sample in `samples[from..to]` and where it is.
fn loudest(samples: &[f32], from: usize, to: usize) -> Option<(usize, f32)> {
    samples
        .get(from..to.min(samples.len()))?
        .iter()
        .enumerate()
        .fold(None, |best: Option<(usize, f32)>, (i, s)| {
            let s = s.abs();
            match best {
                Some((_, b)) if b >= s => best,
                _ => Some((i, s)),
            }
        })
        .map(|(i, s)| (from.saturating_add(i), s))
}

/// The sample where the attack detected at `at` starts.
///
/// Its peak is the loudest sample from [`BEFORE`] ahead of `at` to
/// [`AFTER`] after; the start is the latest sample before that peak
/// where a [`FINE`]-wide envelope is under a tenth of it, at most
/// [`RISE`] back. `at` itself when there is nothing there.
#[must_use]
pub fn attack_start(samples: &[f32], rate: f64, at: usize) -> usize {
    let from = at.saturating_sub(super::samples(BEFORE, rate));
    let to = at.saturating_add(super::samples(AFTER, rate));
    let Some((peak_at, peak)) = loudest(samples, from, to) else {
        return at;
    };
    if peak <= 0.0 {
        return at;
    }
    let window = super::samples(FINE, rate).max(1);
    let limit = peak_at.saturating_sub(super::samples(RISE, rate));
    let floor = START * peak;
    let mut i = peak_at;
    while i > limit {
        let lo = i.saturating_sub(window);
        let env = samples
            .get(lo..i)
            .map_or(0.0, |w| w.iter().fold(0.0_f32, |m, s| m.max(s.abs())));
        if env < floor {
            return i.saturating_sub(window / 2);
        }
        i = i.saturating_sub(1);
    }
    limit
}

/// The attack's peak: the loudest sample in the [`MEASURE`] from `at`,
/// linear.
#[must_use]
pub fn attack_peak(samples: &[f32], rate: f64, at: usize) -> f32 {
    loudest(
        samples,
        at,
        at.saturating_add(super::samples(MEASURE, rate)),
    )
    .map_or(0.0, |(_, s)| s)
}
