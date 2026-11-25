//! LISTEN_ALIGN's search: how each mic of a piece should be moved, turned
//! over or phase-shaped (all-pass) so the piece's sum is fullest — scored on the sum,
//! over the hits only.
//!
//! REAPER's Phase Alignment (7.40) moves an item and may flip its polarity
//! to maximise its correlation with a reference item; Sound Radix
//! Auto-Align 2 adds a correction that depends on frequency ("spectral
//! phase"). Here each mic in turn is scored by what it does to the **sum**
//! it feeds, over the hit windows the hit markers give (no bleed, no
//! silence), on three measures:
//!
//! - `level`: the sum's power over the windows;
//! - `low`: its power below [`LOW_HZ`] (where cancelling mics lose a kick's
//!   or a tom's body);
//! - `attack`: the power of its first difference over the first
//!   [`ATTACK_MS`] after the hit — its transients, so a click out of line
//!   counts for how it is heard, not for its little power.
//!
//! each as dB against the sum as it is, added into one `score`. Moving a
//! mic changes the sum only by that mic, so for an offset `d` and a
//! polarity `p` the new sum's power is closed-form:
//! `E(rest) + E(mic shifted) + 2p·<rest, mic shifted>` — one
//! cross-correlation over the lags per measure, no render. Then the mic's
//! phase is shaped by all-pass sections — the `align` block's: up to two
//! RBJ second-order all-pass biquads `{hz, q}` and one first-order `{hz}`,
//! flat in magnitude and without latency, so what they gain is alignment,
//! never EQ — chosen one at a time, each the section that raises the
//! sum's score most, while one gains more than [`CLOSE`].
//!
//! Every function here is honest: plain windows in, figures out — public
//! API, so a test elsewhere can predict what a fix does without
//! re-deriving it:
//!
//! ```ignore
//! use session_engine::listen::align::{apply, Fix, Section, Windows};
//! let w = Windows::new(48_000, 480); // the hit 10 ms into each window
//! let fix = Fix { delay: -136, invert: false, allpass: vec![Section::Second { hz: 63.0, q: 2.0 }] };
//! let mic_as_the_block_sets_it = apply(&w, &mic_windows, &fix);
//! ```

use realfft::RealFftPlanner;

/// Below this, a sum's body, Hz.
pub const LOW_HZ: f64 = 150.0;
/// The attack, from the hit, ms.
pub const ATTACK_MS: f64 = 10.0;
/// The frequencies an all-pass section is tried at, Hz.
pub const ALLPASS_HZ: [f64; 12] = [
    30.0, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0,
];
/// The widths a second-order section is tried at.
pub const ALLPASS_Q: [f64; 5] = [0.5, 0.7, 1.0, 1.4, 2.0];
/// At most this many second-order sections, and one first-order.
pub const SECOND_ORDER_MOST: usize = 2;
/// Two candidates whose scores are this close are as good: the smaller
/// move wins, dB.
pub const CLOSE: f64 = 0.1;

/// An all-pass section, as the `align` block runs it: flat in magnitude,
/// its phase turning through 180° (second order: 360°) around `hz`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Section {
    /// RBJ's second-order all-pass biquad: centre `hz`, width `q`.
    Second { hz: f64, q: f64 },
    /// The first-order all-pass: −90° at `hz`.
    First { hz: f64 },
}

/// One way to set a mic: how late (samples; negative earlier), whether
/// turned over, and its all-pass sections, in order.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Fix {
    pub delay: i64,
    pub invert: bool,
    pub allpass: Vec<Section>,
}

impl Fix {
    /// Leaving the mic as it is.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// How big a move it is, for preferring the smaller of two as good.
    fn size(&self) -> f64 {
        self.delay.unsigned_abs() as f64
            + if self.invert { 0.5 } else { 0.0 }
            + self.allpass.len() as f64 * 4.0
    }
}

/// What a fix does to the sum, dB against the sum as it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gains {
    pub level: f64,
    pub low: f64,
    pub attack: f64,
}

impl Gains {
    /// The three added.
    #[must_use]
    pub fn score(&self) -> f64 {
        self.level + self.low + self.attack
    }
}

/// A fix and what it does.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub fix: Fix,
    pub gains: Gains,
}

/// A biquad's coefficients (RBJ), run over one window from rest.
#[derive(Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
}

impl Biquad {
    fn lowpass(hz: f64, rate: f64) -> Self {
        let w = std::f64::consts::TAU * hz / rate;
        let alpha = w.sin() / (2.0 * std::f64::consts::FRAC_1_SQRT_2);
        let a0 = 1.0 + alpha;
        let c = w.cos();
        Self {
            b: [(1.0 - c) / 2.0 / a0, (1.0 - c) / a0, (1.0 - c) / 2.0 / a0],
            a: [-2.0 * c / a0, (1.0 - alpha) / a0],
        }
    }

    /// RBJ's second-order all-pass.
    fn allpass(hz: f64, q: f64, rate: f64) -> Self {
        let w = std::f64::consts::TAU * hz / rate;
        let alpha = w.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        let c = -2.0 * w.cos() / a0;
        Self {
            b: [(1.0 - alpha) / a0, c, 1.0],
            a: [c, (1.0 - alpha) / a0],
        }
    }

    /// The first-order all-pass, −90° at `hz`.
    fn allpass_first(hz: f64, rate: f64) -> Self {
        let t = (std::f64::consts::PI * hz / rate).tan();
        let k = (t - 1.0) / (t + 1.0);
        Self {
            b: [k, 1.0, 0.0],
            a: [k, 0.0],
        }
    }

    /// A section's filter.
    fn of(section: Section, rate: f64) -> Self {
        match section {
            Section::Second { hz, q } => Self::allpass(hz, q, rate),
            Section::First { hz } => Self::allpass_first(hz, rate),
        }
    }

    fn run(self, x: &[f64]) -> Vec<f64> {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        x.iter()
            .map(|&s| {
                let y = self.b[0] * s + self.b[1] * x1 + self.b[2] * x2
                    - self.a[0] * y1
                    - self.a[1] * y2;
                (x2, x1, y2, y1) = (x1, s, y1, y);
                y
            })
            .collect()
    }
}

/// One measure's view of the windows: the samples it counts.
#[derive(Clone, Copy)]
enum Measure {
    Level,
    Low,
    Attack,
}

/// The windows of a piece, ready to score fixes on.
pub struct Windows {
    rate: f64,
    /// The hit's frame in each window.
    pre: usize,
    attack: usize,
}

impl Windows {
    /// Windows at `rate` whose hit is `pre` frames in.
    #[must_use]
    pub fn new(rate: u32, pre: usize) -> Self {
        let rate = f64::from(rate);
        Self {
            rate,
            pre,
            attack: (ATTACK_MS * rate / 1e3) as usize,
        }
    }

    /// The samples of `x` a measure counts.
    fn view(&self, m: Measure, x: &[f64]) -> Vec<f64> {
        match m {
            Measure::Level => x.to_vec(),
            Measure::Low => Biquad::lowpass(LOW_HZ, self.rate).run(x),
            // Its transients: the first difference over the attack, so a
            // beater's click out of line counts for what it is heard as,
            // not for its little power.
            Measure::Attack => {
                let end = (self.pre + self.attack).min(x.len());
                (self.pre.max(1)..end).map(|i| x[i] - x[i - 1]).collect()
            }
        }
    }
}

fn to64(w: &[Vec<f32>]) -> Vec<Vec<f64>> {
    w.iter()
        .map(|x| x.iter().map(|v| f64::from(*v)).collect())
        .collect()
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn db(ratio: f64) -> f64 {
    if ratio <= 1e-15 {
        -150.0
    } else {
        10.0 * ratio.log10()
    }
}

/// `x` shifted `d` frames later (zero where it runs off).
fn shift(x: &[f64], d: i64) -> Vec<f64> {
    (0..x.len())
        .map(|i| {
            usize::try_from(i as i64 - d)
                .ok()
                .and_then(|j| x.get(j))
                .copied()
                .unwrap_or(0.0)
        })
        .collect()
}

/// Every offset within `most` frames, each way, and both polarities
/// (`invert` allowed), scored on the sum: what moving `mic` to each does
/// to `sum` (windows, the mic in the sum as it is). Best first; within
/// [`CLOSE`] of each other the smaller move first.
#[must_use]
pub fn offsets(
    w: &Windows,
    mic: &[Vec<f32>],
    sum: &[Vec<f32>],
    (most, invert): (i64, bool),
) -> Vec<Candidate> {
    use rayon::prelude::*;
    let (mic, sum) = (to64(mic), to64(sum));
    let measures = [Measure::Level, Measure::Low, Measure::Attack];
    // Per measure, per window: the rest (the sum without the mic) and the
    // mic as the measure sees it. A shifted mic's view is the view shifted
    // (filters are linear and time-invariant; the attack view shifts the
    // mic before cutting).
    let views: Vec<(Vec<Vec<f64>>, Vec<Vec<f64>>)> = measures
        .iter()
        .map(|&m| {
            let rest: Vec<Vec<f64>> = sum
                .iter()
                .zip(&mic)
                .map(|(s, x)| w.view(m, &s.iter().zip(x).map(|(a, b)| a - b).collect::<Vec<_>>()))
                .collect();
            let mic_full: Vec<Vec<f64>> = mic
                .iter()
                .map(|x| match m {
                    Measure::Attack => x.clone(),
                    _ => w.view(m, x),
                })
                .collect();
            (rest, mic_full)
        })
        .collect();
    let base: Vec<f64> = measures
        .iter()
        .map(|&m| {
            sum.iter()
                .map(|s| {
                    let v = w.view(m, s);
                    dot(&v, &v)
                })
                .sum()
        })
        .collect();
    let mut out: Vec<Candidate> = (-most..=most)
        .into_par_iter()
        .flat_map_iter(|d| {
            let (mut e_rest, mut e_mic, mut cross) = ([0.0; 3], [0.0; 3], [0.0; 3]);
            for (k, &m) in measures.iter().enumerate() {
                let (rest, mic_full) = &views[k];
                for (r, x) in rest.iter().zip(mic_full) {
                    let moved = match m {
                        Measure::Attack => w.view(m, &shift(x, d)),
                        _ => shift(x, d),
                    };
                    e_rest[k] += dot(r, r);
                    e_mic[k] += dot(&moved, &moved);
                    cross[k] += dot(r, &moved);
                }
            }
            let polarities: &[f64] = if invert { &[1.0, -1.0] } else { &[1.0] };
            let base = &base;
            polarities.iter().map(move |&p| {
                let gain =
                    |k: usize| db((e_rest[k] + e_mic[k] + 2.0 * p * cross[k]) / base[k].max(1e-30));
                Candidate {
                    fix: Fix {
                        delay: d,
                        invert: p < 0.0,
                        allpass: Vec::new(),
                    },
                    gains: Gains {
                        level: gain(0),
                        low: gain(1),
                        attack: gain(2),
                    },
                }
            })
        })
        .collect();
    rank(&mut out);
    out
}

/// Best first; within [`CLOSE`] of each other, the smaller move first.
fn rank(c: &mut [Candidate]) {
    c.sort_by(|a, b| b.gains.score().total_cmp(&a.gains.score()));
    let Some(best) = c.first().map(|c| c.gains.score()) else {
        return;
    };
    let close = c
        .iter()
        .take_while(|x| best - x.gains.score() <= CLOSE)
        .count();
    c[..close].sort_by(|a, b| a.fix.size().total_cmp(&b.fix.size()));
}

/// `mic` (windows) as `fix` sets it.
#[must_use]
pub fn apply(w: &Windows, mic: &[Vec<f32>], fix: &Fix) -> Vec<Vec<f32>> {
    let p = if fix.invert { -1.0 } else { 1.0 };
    mic.iter()
        .map(|x| {
            let x: Vec<f64> = x.iter().map(|v| f64::from(*v) * p).collect();
            let x = shift(&x, fix.delay);
            let x = fix
                .allpass
                .iter()
                .fold(x, |x, s| Biquad::of(*s, w.rate).run(&x));
            x.iter().map(|v| *v as f32).collect()
        })
        .collect()
}

/// All-pass sections for `mic` (already set by `fix`, with none), chosen
/// one at a time: each stage tries every second-order section of
/// [`ALLPASS_HZ`] × [`ALLPASS_Q`] (while fewer than
/// [`SECOND_ORDER_MOST`]) and every first-order one (while there is none),
/// scored on the sum as [`offsets`] scores, and keeps the best while it
/// gains more than [`CLOSE`] on the stage before. The fix with them, and
/// what it does (the stage-0 fix as it was when none gains).
#[must_use]
pub fn allpass(w: &Windows, mic: &[Vec<f32>], sum: &[Vec<f32>], fix: &Fix) -> Candidate {
    use rayon::prelude::*;
    let rest: Vec<Vec<f32>> = sum
        .iter()
        .zip(mic)
        .map(|(s, m)| s.iter().zip(m).map(|(a, b)| a - b).collect())
        .collect();
    let score_of = |f: &Fix| {
        let set = apply(w, mic, f);
        let new_sum: Vec<Vec<f32>> = rest
            .iter()
            .zip(&set)
            .map(|(r, m)| r.iter().zip(m).map(|(a, b)| a + b).collect())
            .collect();
        compare(w, sum, &new_sum)
    };
    let mut best = Candidate {
        fix: fix.clone(),
        gains: score_of(fix),
    };
    loop {
        let seconds = best
            .fix
            .allpass
            .iter()
            .filter(|s| matches!(s, Section::Second { .. }))
            .count();
        let has_first = best
            .fix
            .allpass
            .iter()
            .any(|s| matches!(s, Section::First { .. }));
        let mut options: Vec<Section> = Vec::new();
        if seconds < SECOND_ORDER_MOST {
            options.extend(
                ALLPASS_HZ
                    .iter()
                    .flat_map(|&hz| ALLPASS_Q.iter().map(move |&q| Section::Second { hz, q })),
            );
        }
        if !has_first {
            options.extend(ALLPASS_HZ.iter().map(|&hz| Section::First { hz }));
        }
        let next = options
            .par_iter()
            .map(|s| {
                let mut f = best.fix.clone();
                f.allpass.push(*s);
                let gains = score_of(&f);
                Candidate { fix: f, gains }
            })
            .max_by(|a, b| a.gains.score().total_cmp(&b.gains.score()));
        match next {
            Some(n) if n.gains.score() > best.gains.score() + CLOSE => best = n,
            _ => return best,
        }
    }
}

/// The magnitude of `fix`'s filtering at `hz`, dB: 0 for every section
/// (all-pass), kept as the check that it is.
#[must_use]
pub fn magnitude_db(fix: &Fix, hz: f64, rate: u32) -> f64 {
    use realfft::num_complex::Complex;
    let w = std::f64::consts::TAU * hz / f64::from(rate);
    let z1 = Complex::from_polar(1.0, -w);
    let z2 = z1 * z1;
    fix.allpass
        .iter()
        .map(|s| {
            let b = Biquad::of(*s, f64::from(rate));
            let h = (b.b[0] + b.b[1] * z1 + b.b[2] * z2) / (1.0 + b.a[0] * z1 + b.a[1] * z2);
            20.0 * h.norm().log10()
        })
        .sum()
}

/// What `after` (windows) gains over `before` on each measure, dB.
#[must_use]
pub fn compare(w: &Windows, before: &[Vec<f32>], after: &[Vec<f32>]) -> Gains {
    let power = |m: Measure, x: &[Vec<f32>]| -> f64 {
        to64(x)
            .iter()
            .map(|v| {
                let v = w.view(m, v);
                dot(&v, &v)
            })
            .sum()
    };
    let gain = |m: Measure| db(power(m, after) / power(m, before).max(1e-30));
    Gains {
        level: gain(Measure::Level),
        low: gain(Measure::Low),
        attack: gain(Measure::Attack),
    }
}

/// `sum` with `mic` (its windows as they are in it) replaced by `fixed`.
#[must_use]
pub fn swap(sum: &[Vec<f32>], mic: &[Vec<f32>], fixed: &[Vec<f32>]) -> Vec<Vec<f32>> {
    sum.iter()
        .zip(mic)
        .zip(fixed)
        .map(|((s, m), f)| {
            s.iter()
                .zip(m)
                .zip(f)
                .map(|((s, m), f)| s - m + f)
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// A kick: a 60 Hz thump and a 2 kHz click, `at` frames into a window
    /// `len` long.
    fn thump(len: usize, at: usize, gain: f32) -> Vec<f32> {
        (0..len)
            .map(|i| {
                if i < at {
                    return 0.0;
                }
                let t = (i - at) as f32 / RATE as f32;
                // A body and a beater's click.
                gain * ((std::f32::consts::TAU * 60.0 * t).sin() * (-t * 15.0).exp()
                    + 0.5 * (std::f32::consts::TAU * 2_000.0 * t).sin() * (-t * 400.0).exp())
            })
            .collect()
    }

    #[test]
    fn a_mic_late_and_upside_down_is_found_moved_back_and_turned_over() {
        let w = Windows::new(RATE, 480);
        let a: Vec<Vec<f32>> = (0..4).map(|_| thump(9_600, 480, 0.5)).collect();
        // The other mic: 20 samples late and inverted.
        let b: Vec<Vec<f32>> = (0..4)
            .map(|_| thump(9_600, 500, 0.5).iter().map(|s| -s).collect())
            .collect();
        let sum: Vec<Vec<f32>> = a
            .iter()
            .zip(&b)
            .map(|(x, y)| x.iter().zip(y).map(|(p, q)| p + q).collect())
            .collect();
        let best = offsets(&w, &b, &sum, (48, true))[0].clone();
        assert_eq!(best.fix.delay, -20, "{best:?}");
        assert!(best.fix.invert, "{best:?}");
        assert!(
            best.gains.low > 6.0,
            "a cancelling pair made whole: {best:?}"
        );
        // Applied, the sum is what the search said it would be.
        let fixed = apply(&w, &b, &best.fix);
        let new_sum: f64 = a
            .iter()
            .zip(&fixed)
            .flat_map(|(x, y)| x.iter().zip(y).map(|(p, q)| f64::from(p + q).powi(2)))
            .sum();
        let old: f64 = sum.iter().flatten().map(|v| f64::from(*v).powi(2)).sum();
        assert!(
            (db(new_sum / old) - best.gains.level).abs() < 0.05,
            "{best:?}"
        );
    }

    #[test]
    fn a_mic_already_right_stays_where_it_is() {
        let w = Windows::new(RATE, 480);
        let a: Vec<Vec<f32>> = (0..3).map(|_| thump(4_800, 480, 0.5)).collect();
        let sum: Vec<Vec<f32>> = a
            .iter()
            .map(|x| x.iter().map(|v| v * 2.0).collect())
            .collect();
        let best = offsets(&w, &a, &sum, (48, true))[0].clone();
        assert_eq!(best.fix, Fix::none(), "{best:?}");
    }

    #[test]
    fn every_all_pass_section_is_flat_and_one_puts_a_phase_shifted_mic_back() {
        let flat = Fix {
            allpass: vec![
                Section::Second { hz: 63.0, q: 0.7 },
                Section::Second { hz: 200.0, q: 2.0 },
                Section::First { hz: 100.0 },
            ],
            ..Fix::none()
        };
        for hz in [20.0, 40.0, 63.0, 100.0, 1_000.0, 10_000.0] {
            assert!(magnitude_db(&flat, hz, RATE).abs() < 1e-6, "{hz} Hz");
        }
        let w = Windows::new(RATE, 480);
        let a: Vec<Vec<f32>> = (0..3).map(|_| thump(9_600, 480, 0.5)).collect();
        // The other mic: the same kick through a 60 Hz second-order
        // all-pass (its body turned about 180°, cancelling the first).
        let b = apply(
            &w,
            &a,
            &Fix {
                allpass: vec![Section::Second { hz: 60.0, q: 1.0 }],
                ..Fix::none()
            },
        );
        let sum: Vec<Vec<f32>> = a
            .iter()
            .zip(&b)
            .map(|(x, y)| x.iter().zip(y).map(|(p, q)| p + q).collect())
            .collect();
        let best = allpass(&w, &b, &sum, &Fix::none());
        assert!(!best.fix.allpass.is_empty(), "{best:?}");
        assert!(best.gains.low > 3.0, "the body back in phase: {best:?}");
    }
}
