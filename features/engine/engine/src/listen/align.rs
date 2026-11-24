//! LISTEN_ALIGN's search: how each mic of a piece should be moved, turned
//! over or phase-rotated so the piece's sum is fullest — scored on the sum,
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
//! cross-correlation over the lags per measure, no render. A rotation of
//! one band of the mic (centre `hz`, width `q`) by an angle `θ` replaces
//! that band `b` with `cos θ·b + sin θ·H(b)` (`H` its Hilbert transform):
//! the sum's power is again closed-form in `θ`, so each centre is solved
//! exactly.
//!
//! Every function here is honest: plain windows in, figures out.

use realfft::RealFftPlanner;

/// Below this, a sum's body, Hz.
pub const LOW_HZ: f64 = 150.0;
/// The attack, from the hit, ms.
pub const ATTACK_MS: f64 = 10.0;
/// The band centres a rotation is tried at, Hz.
pub const ROTATE_AT: [f64; 9] = [40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0];
/// A rotated band's width.
pub const ROTATE_Q: f64 = 1.0;
/// Two candidates whose scores are this close are as good: the smaller
/// move wins, dB.
pub const CLOSE: f64 = 0.1;

/// A band rotation: the mic's band around `hz` (width `q`) turned by `deg`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotation {
    pub hz: f64,
    pub deg: f64,
    pub q: f64,
}

/// One way to set a mic: how late (samples; negative earlier), whether
/// turned over, and a band rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fix {
    pub delay: i64,
    pub invert: bool,
    pub rotate: Option<Rotation>,
}

impl Fix {
    pub const NONE: Self = Self {
        delay: 0,
        invert: false,
        rotate: None,
    };

    /// How big a move it is, for preferring the smaller of two as good.
    fn size(&self) -> f64 {
        self.delay.unsigned_abs() as f64
            + if self.invert { 0.5 } else { 0.0 }
            + self.rotate.map_or(0.0, |r| r.deg.abs() / 90.0)
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
#[derive(Clone, Copy, Debug, PartialEq)]
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

    /// Band-pass, 0 dB at its centre.
    fn bandpass(hz: f64, q: f64, rate: f64) -> Self {
        let w = std::f64::consts::TAU * hz / rate;
        let alpha = w.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b: [alpha / a0, 0.0, -alpha / a0],
            a: [-2.0 * w.cos() / a0, (1.0 - alpha) / a0],
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

/// The Hilbert transform of one window (its every component turned by
/// −90°).
fn hilbert(x: &[f64]) -> Vec<f64> {
    let n = x.len().next_power_of_two().max(2);
    let mut planner = RealFftPlanner::<f64>::new();
    let (fwd, inv) = (planner.plan_fft_forward(n), planner.plan_fft_inverse(n));
    let mut input = fwd.make_input_vec();
    input[..x.len()].copy_from_slice(x);
    let mut spec = fwd.make_output_vec();
    if fwd.process(&mut input, &mut spec).is_err() {
        return vec![0.0; x.len()];
    }
    let last = spec.len() - 1;
    for (k, c) in spec.iter_mut().enumerate() {
        *c = if k == 0 || k == last {
            realfft::num_complex::Complex::new(0.0, 0.0)
        } else {
            // × −i
            realfft::num_complex::Complex::new(c.im, -c.re)
        };
    }
    let mut out = inv.make_output_vec();
    if inv.process(&mut spec, &mut out).is_err() {
        return vec![0.0; x.len()];
    }
    out.truncate(x.len());
    out.iter().map(|v| v / n as f64).collect()
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
                        rotate: None,
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
            let x = match fix.rotate {
                None => x,
                Some(r) => {
                    let b = Biquad::bandpass(r.hz, r.q, w.rate).run(&x);
                    let h = hilbert(&b);
                    let (c, s) = (r.deg.to_radians().cos(), r.deg.to_radians().sin());
                    x.iter()
                        .zip(&b)
                        .zip(&h)
                        .map(|((x, b), h)| x - b + c * b + s * h)
                        .collect()
                }
            };
            x.iter().map(|v| *v as f32).collect()
        })
        .collect()
}

/// The best band rotation of `mic` (already set by `fix`, without one) at
/// each of [`ROTATE_AT`], scored as [`offsets`] scores: per centre, the
/// angle solved exactly for the sum's power, then scored on all three
/// measures. Best first.
#[must_use]
pub fn rotations(w: &Windows, mic: &[Vec<f32>], sum: &[Vec<f32>], fix: &Fix) -> Vec<Candidate> {
    use rayon::prelude::*;
    let set = to64(&apply(w, mic, fix));
    let (mic0, sum) = (to64(mic), to64(sum));
    // The rest: the sum without the mic as it is.
    let rest: Vec<Vec<f64>> = sum
        .iter()
        .zip(&mic0)
        .map(|(s, m)| s.iter().zip(m).map(|(a, b)| a - b).collect())
        .collect();
    let measures = [Measure::Level, Measure::Low, Measure::Attack];
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
    let mut out: Vec<Candidate> = ROTATE_AT
        .par_iter()
        .map(|&hz| {
            // Per window: u = rest + mic - band, the band, its Hilbert.
            let parts: Vec<(Vec<f64>, Vec<f64>, Vec<f64>)> = rest
                .iter()
                .zip(&set)
                .map(|(r, x)| {
                    let b = Biquad::bandpass(hz, ROTATE_Q, w.rate).run(x);
                    let h = hilbert(&b);
                    let u = r
                        .iter()
                        .zip(x)
                        .zip(&b)
                        .map(|((r, x), b)| r + x - b)
                        .collect();
                    (u, b, h)
                })
                .collect();
            // E(θ) per measure: E(u) + c²E(b) + s²E(h) + 2c<u,b> + 2s<u,h> + 2cs<b,h>.
            let terms: Vec<[f64; 6]> = measures
                .iter()
                .map(|&m| {
                    parts.iter().fold([0.0; 6], |mut t, (u, b, h)| {
                        let (u, b, h) = (w.view(m, u), w.view(m, b), w.view(m, h));
                        t[0] += dot(&u, &u);
                        t[1] += dot(&b, &b);
                        t[2] += dot(&h, &h);
                        t[3] += dot(&u, &b);
                        t[4] += dot(&u, &h);
                        t[5] += dot(&b, &h);
                        t
                    })
                })
                .collect();
            let power = |t: &[f64; 6], deg: f64| {
                let (c, s) = (deg.to_radians().cos(), deg.to_radians().sin());
                t[0] + c * c * t[1]
                    + s * s * t[2]
                    + 2.0 * c * t[3]
                    + 2.0 * s * t[4]
                    + 2.0 * c * s * t[5]
            };
            let gains_at = |deg: f64| Gains {
                level: db(power(&terms[0], deg) / base[0].max(1e-30)),
                low: db(power(&terms[1], deg) / base[1].max(1e-30)),
                attack: db(power(&terms[2], deg) / base[2].max(1e-30)),
            };
            // The angle, to a degree, that scores best.
            let deg = (-179..=180)
                .map(f64::from)
                .max_by(|a, b| gains_at(*a).score().total_cmp(&gains_at(*b).score()))
                .unwrap_or(0.0);
            Candidate {
                fix: Fix {
                    rotate: Some(Rotation {
                        hz,
                        deg,
                        q: ROTATE_Q,
                    }),
                    ..*fix
                },
                gains: gains_at(deg),
            }
        })
        .collect();
    rank(&mut out);
    out
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
        let best = offsets(&w, &b, &sum, (48, true))[0];
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
        let best = offsets(&w, &a, &sum, (48, true))[0];
        assert_eq!(best.fix, Fix::NONE, "{best:?}");
    }

    #[test]
    fn a_band_rotated_against_the_rest_is_turned_back() {
        let w = Windows::new(RATE, 480);
        let a: Vec<Vec<f32>> = (0..3).map(|_| thump(9_600, 480, 0.5)).collect();
        // The same thump, its 60 Hz turned by 120°: a rotation near 63 Hz
        // by about -120° undoes it, the sum gaining.
        let turn = Fix {
            delay: 0,
            invert: false,
            rotate: Some(Rotation {
                hz: 63.0,
                deg: 120.0,
                q: ROTATE_Q,
            }),
        };
        let b = apply(&w, &a, &turn);
        let sum: Vec<Vec<f32>> = a
            .iter()
            .zip(&b)
            .map(|(x, y)| x.iter().zip(y).map(|(p, q)| p + q).collect())
            .collect();
        let best = rotations(&w, &b, &sum, &Fix::NONE)[0];
        let r = best.fix.rotate.expect("a rotation");
        assert!(r.deg < -60.0, "{best:?}");
        assert!(best.gains.low > 0.5, "{best:?}");
    }
}
