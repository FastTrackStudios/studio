//! What MIX_MEASURE says about a stereo signal: its loudness (ITU-R
//! BS.1770-4 / EBU R128: integrated and short-term), its true peak, its
//! RMS and crest factor, its spectrum in octave or third-octave bands, and
//! its stereo width.
//!
//! Every level is in dB against full scale, floored at [`FLOOR`] for
//! silence. RMS is the plain RMS of both channels (a full-scale sine reads
//! −3.01 dB, not the AES17 0 dB); a band's level is the RMS the signal has
//! inside it, so a sine reads its own RMS in its band.

use realfft::RealFftPlanner;

/// What silence reads, dB.
pub const FLOOR: f64 = -150.0;

/// `10·log10(x)`, floored.
fn power_db(x: f64) -> f64 {
    if x <= 1e-15 {
        FLOOR
    } else {
        (10.0 * x.log10()).max(FLOOR)
    }
}

/// `20·log10(x)`, floored.
fn amp_db(x: f64) -> f64 {
    if x <= 1e-15 {
        FLOOR
    } else {
        (20.0 * x.log10()).max(FLOOR)
    }
}

/// Which bands a spectrum is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bands {
    /// The ten octaves, 31.5 Hz … 16 kHz.
    Octave,
    /// The thirty-one thirds, 20 Hz … 20 kHz.
    Third,
}

impl Bands {
    /// Each band's centre, Hz.
    #[must_use]
    pub fn centres(self) -> Vec<f64> {
        match self {
            Self::Octave => (0..10).map(|i| 31.25 * 2f64.powi(i)).collect(),
            // Base-2 thirds about 1 kHz: 1000·2^(k/3), k = −17 … 13.
            Self::Third => (-17..=13)
                .map(|k| 1000.0 * 2f64.powf(f64::from(k) / 3.0))
                .collect(),
        }
    }

    /// The ratio from a band's centre to its edges.
    fn half_width(self) -> f64 {
        match self {
            Self::Octave => 2f64.sqrt(),
            Self::Third => 2f64.powf(1.0 / 6.0),
        }
    }
}

/// One signal's measurement.
#[derive(Clone, Debug, PartialEq)]
pub struct Stats {
    /// Integrated loudness, LUFS (gated).
    pub lufs: f64,
    /// The loudest 3 s window, LUFS (the whole span when it is shorter).
    pub short_max: f64,
    /// True peak, dBTP (4× oversampled).
    pub true_peak: f64,
    /// Sample peak, dBFS.
    pub peak: f64,
    /// RMS of both channels, dBFS.
    pub rms: f64,
    /// Peak over RMS, dB.
    pub crest: f64,
    /// Each band's level (as [`Bands::centres`]), dB.
    pub bands: Vec<f64>,
    /// Left–right correlation, −1 … 1 (1 for mono; 0 for silence).
    pub correlation: f64,
    /// Side over mid, as a level ratio (0 mono, 1 as wide as it is deep).
    pub width: f64,
    /// Left over right, dB.
    pub balance: f64,
    /// The K-weighted mean square over the span (what a share of a sum
    /// compares).
    pub k_power: f64,
}

/// A biquad, direct form I, run over a whole signal.
#[derive(Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
}

/// Whole-signal runs: what [`k_sums`] is held to (the tests).
#[cfg(test)]
impl Biquad {
    fn run(self, x: &[f32]) -> Vec<f64> {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        x.iter()
            .map(|&s| {
                let s = f64::from(s);
                let y = self.b[0] * s + self.b[1] * x1 + self.b[2] * x2
                    - self.a[0] * y1
                    - self.a[1] * y2;
                (x2, x1, y2, y1) = (x1, s, y1, y);
                y
            })
            .collect()
    }

    fn run_f64(self, x: &[f64]) -> Vec<f64> {
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

/// BS.1770's K-weighting at `rate`: its high shelf, then its high-pass
/// (the coefficients worked out for the rate, as libebur128 does).
fn k_filters(rate: f64) -> (Biquad, Biquad) {
    use std::f64::consts::PI;
    let (f0, g, q) = (
        1_681.974_450_955_533,
        3.999_843_853_973_347,
        0.707_175_236_955_419_6,
    );
    let k = (PI * f0 / rate).tan();
    let vh = 10f64.powf(g / 20.0);
    let vb = vh.powf(0.499_666_774_154_541_6);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Biquad {
        b: [
            (vh + vb * k / q + k * k) / a0,
            2.0 * (k * k - vh) / a0,
            (vh - vb * k / q + k * k) / a0,
        ],
        a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    };
    let (f0, q) = (38.135_470_876_024_44, 0.500_327_037_323_877_3);
    let k = (PI * f0 / rate).tan();
    let a0 = 1.0 + k / q + k * k;
    let high = Biquad {
        b: [1.0, -2.0, 1.0],
        a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    };
    (shelf, high)
}

/// Each channel K-weighted.
#[cfg(test)]
fn k_weighted(ch: &[f32], rate: f64) -> Vec<f64> {
    let (shelf, high) = k_filters(rate);
    high.run_f64(&shelf.run(ch))
}

/// One biquad's running state, a sample at a time: what [`Biquad::run`]
/// computes, without the signal it would collect.
#[derive(Default)]
struct Stage {
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Stage {
    #[inline]
    fn step(&mut self, f: &Biquad, s: f64) -> f64 {
        let y =
            f.b[0] * s + f.b[1] * self.x1 + f.b[2] * self.x2 - f.a[0] * self.y1 - f.a[1] * self.y2;
        (self.x2, self.x1, self.y2, self.y1) = (self.x1, s, self.y1, y);
        y
    }
}

/// The running sums of both channels' K-weighted squares (`acc[i]`, the
/// first `i` samples; `acc[0] = 0`): every window's power is a difference
/// of two. One pass, no K-weighted signal kept: the same arithmetic, in
/// the same order, as K-weighting each channel and then summing.
fn k_sums(l: &[f32], r: &[f32], rate: f64) -> Vec<f64> {
    let (shelf, high) = k_filters(rate);
    let n = l.len().min(r.len());
    let (mut ls, mut lh, mut rs, mut rh) = (
        Stage::default(),
        Stage::default(),
        Stage::default(),
        Stage::default(),
    );
    let mut acc = Vec::with_capacity(n + 1);
    acc.push(0.0);
    let mut sum = 0.0;
    for (&a, &b) in l[..n].iter().zip(&r[..n]) {
        let kl = lh.step(&high, ls.step(&shelf, f64::from(a)));
        let kr = rh.step(&high, rs.step(&shelf, f64::from(b)));
        sum += kl * kl + kr * kr;
        acc.push(sum);
    }
    acc
}

/// Loudness of a mean square (summed over channels), LUFS.
fn lufs_of(power: f64) -> f64 {
    if power <= 1e-15 {
        FLOOR
    } else {
        (-0.691 + 10.0 * power.log10()).max(FLOOR)
    }
}

/// Each window's K-weighted power (summed over both channels), `window`
/// samples long every `hop`, from the running sums of squares ([`k_sums`]):
/// each window in O(1).
fn window_powers(acc: &[f64], window: usize, hop: usize) -> Vec<f64> {
    let n = acc.len().saturating_sub(1);
    if n == 0 || window == 0 {
        return Vec::new();
    }
    if n < window {
        return vec![acc[n] / n as f64];
    }
    (0..=(n - window))
        .step_by(hop.max(1))
        .map(|s| (acc[s + window] - acc[s]) / window as f64)
        .collect()
}

/// BS.1770's gated integration of 400 ms blocks.
fn integrated(blocks: &[f64]) -> f64 {
    let absolute: Vec<f64> = blocks
        .iter()
        .copied()
        .filter(|&p| lufs_of(p) > -70.0)
        .collect();
    if absolute.is_empty() {
        return FLOOR;
    }
    let mean = absolute.iter().sum::<f64>() / absolute.len() as f64;
    let gate = lufs_of(mean) - 10.0;
    let relative: Vec<f64> = absolute
        .into_iter()
        .filter(|&p| lufs_of(p) > gate)
        .collect();
    if relative.is_empty() {
        return FLOOR;
    }
    lufs_of(relative.iter().sum::<f64>() / relative.len() as f64)
}

/// The 4× oversampling interpolator: a windowed sinc, 12 taps a phase.
fn true_peak(ch: &[f32]) -> f64 {
    const PHASES: usize = 4;
    const TAPS: usize = 12;
    let half = (TAPS / 2) as f64;
    let mut kernel = [[0.0f64; TAPS]; PHASES];
    for (p, phase) in kernel.iter_mut().enumerate() {
        let frac = p as f64 / PHASES as f64;
        for (t, tap) in phase.iter_mut().enumerate() {
            let x = t as f64 - half + 1.0 - frac;
            let sinc = if x.abs() < 1e-12 {
                1.0
            } else {
                (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
            };
            // Hann over the kernel's span.
            let w = 0.5 * (1.0 + (std::f64::consts::PI * x / (half + 1.0)).cos());
            *tap = sinc * w;
        }
    }
    let mut peak = 0.0f64;
    for i in 0..ch.len() {
        peak = peak.max(f64::from(ch[i]).abs());
        if i + TAPS > ch.len() {
            continue;
        }
        let window = &ch[i..i + TAPS];
        for phase in &kernel[1..] {
            let y: f64 = window
                .iter()
                .zip(phase)
                .map(|(s, k)| f64::from(*s) * k)
                .sum();
            peak = peak.max(y.abs());
        }
    }
    peak
}

/// Each band's level of the mid signal: a Welch average of Hann-windowed
/// 8192-point spectra, half overlapped.
pub fn spectrum(l: &[f32], r: &[f32], rate: f64, bands: Bands) -> Vec<f64> {
    const N: usize = 8_192;
    let centres = bands.centres();
    let n = l.len().min(r.len());
    let mid: Vec<f32> = (0..n).map(|i| 0.5 * (l[i] + r[i])).collect();
    // A span shorter than one window is zero-padded to it.
    let size = if n >= N {
        N
    } else {
        n.next_power_of_two().max(256)
    };
    let window: Vec<f64> = (0..size)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / size as f64).cos())
        .collect();
    let window_power = window.iter().map(|w| w * w).sum::<f64>() / size as f64;
    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(size);
    let mut input = fft.make_input_vec();
    let mut output = fft.make_output_vec();
    let mut power = vec![0.0f64; size / 2 + 1];
    let mut frames = 0usize;
    let mut start = 0usize;
    loop {
        for (i, x) in input.iter_mut().enumerate() {
            *x = mid.get(start + i).map_or(0.0, |s| f64::from(*s)) * window[i];
        }
        if fft.process(&mut input, &mut output).is_err() {
            break;
        }
        for (p, c) in power.iter_mut().zip(&output) {
            *p += c.norm_sqr();
        }
        frames += 1;
        start += size / 2;
        if start + size > n {
            break;
        }
    }
    let frames = frames.max(1) as f64;
    // A bin's share of the mean square: both sides of the spectrum but at
    // DC and Nyquist, the window's power taken out.
    let scale = 1.0 / (size as f64 * size as f64 * window_power * frames);
    let bin_hz = rate / size as f64;
    // The span a zero-padded short window carries is less than `size`.
    let padded = if n < size {
        size as f64 / n.max(1) as f64
    } else {
        1.0
    };
    let edge = bands.half_width();
    centres
        .iter()
        .map(|&c| {
            let (lo, hi) = (c / edge, c * edge);
            let ms: f64 = power
                .iter()
                .enumerate()
                .filter(|(k, _)| {
                    let f = *k as f64 * bin_hz;
                    f >= lo && f < hi
                })
                .map(|(k, p)| {
                    let both = if k == 0 || k == size / 2 { 1.0 } else { 2.0 };
                    p * both * scale
                })
                .sum();
            power_db(ms * padded)
        })
        .collect()
}

/// The short-term loudness (3 s windows, BS.1770) every `hop` seconds,
/// LUFS: one value for a signal shorter than a window.
#[must_use]
pub fn short_term(l: &[f32], r: &[f32], rate: u32, hop: f64) -> Vec<f64> {
    let rate_f = f64::from(rate);
    let acc = k_sums(l, r, rate_f);
    let samples = |s: f64| (s * rate_f).round() as usize;
    window_powers(&acc, samples(3.0), samples(hop))
        .into_iter()
        .map(|p| (lufs_of(p) * 10.0).round() / 10.0)
        .collect()
}

/// Measure one stereo signal at `rate`.
#[must_use]
pub fn measure(l: &[f32], r: &[f32], rate: u32, bands: Bands) -> Stats {
    let rate_f = f64::from(rate);
    let n = l.len().min(r.len());
    let (l, r) = (&l[..n], &r[..n]);
    let acc = k_sums(l, r, rate_f);
    let samples = |s: f64| (s * rate_f).round() as usize;
    let blocks = window_powers(&acc, samples(0.4), samples(0.1));
    let short = window_powers(&acc, samples(3.0), samples(0.1));
    let short_max = short.iter().copied().fold(0.0, f64::max);
    let k_power = if n == 0 { 0.0 } else { acc[n] / n as f64 };
    drop(acc);
    let (mut sl, mut sr, mut slr, mut peak) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let (mut mid, mut side) = (0.0f64, 0.0f64);
    for i in 0..n {
        let (a, b) = (f64::from(l[i]), f64::from(r[i]));
        sl += a * a;
        sr += b * b;
        slr += a * b;
        peak = peak.max(a.abs()).max(b.abs());
        mid += (0.5 * (a + b)).powi(2);
        side += (0.5 * (a - b)).powi(2);
    }
    let rms = if n == 0 {
        0.0
    } else {
        ((sl + sr) / (2 * n) as f64).sqrt()
    };
    let correlation = if sl * sr > 1e-24 {
        slr / (sl * sr).sqrt()
    } else {
        0.0
    };
    let width = if mid > 1e-24 {
        (side / mid).sqrt()
    } else {
        0.0
    };
    let balance = if sl > 1e-24 && sr > 1e-24 {
        power_db(sl / sr)
    } else {
        0.0
    };
    let true_peak = true_peak(l).max(true_peak(r));
    let round = |x: f64| (x * 100.0).round() / 100.0;
    Stats {
        lufs: round(integrated(&blocks)),
        short_max: round(lufs_of(short_max)),
        true_peak: round(amp_db(true_peak)),
        peak: round(amp_db(peak)),
        rms: round(amp_db(rms)),
        crest: round(if rms > 0.0 {
            amp_db(peak) - amp_db(rms)
        } else {
            0.0
        }),
        bands: spectrum(l, r, rate_f, bands)
            .into_iter()
            .map(round)
            .collect(),
        correlation: round(correlation),
        width: round(width),
        balance: round(balance),
        k_power,
    }
}

/// `part`'s share of `whole`, dB (both K-weighted powers).
#[must_use]
pub fn share_db(part: f64, whole: f64) -> f64 {
    if whole <= 1e-15 {
        return FLOOR;
    }
    ((power_db(part / whole)) * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn sine(hz: f64, rms_db: f64, seconds: f64) -> Vec<f32> {
        let amp = 10f64.powf(rms_db / 20.0) * 2f64.sqrt();
        (0..(seconds * f64::from(RATE)) as usize)
            .map(|i| {
                (amp * (2.0 * std::f64::consts::PI * hz * i as f64 / f64::from(RATE)).sin()) as f32
            })
            .collect()
    }

    #[test]
    fn a_sine_reads_its_rms_and_its_band() {
        let s = sine(1_000.0, -18.0, 4.0);
        let m = measure(&s, &s, RATE, Bands::Octave);
        assert!((m.rms - -18.0).abs() < 0.05, "{m:?}");
        assert!((m.crest - 3.01).abs() < 0.05, "{m:?}");
        // 1 kHz is the octave band centred on 1000 (the sixth).
        let loudest = m
            .bands
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i);
        assert_eq!(loudest, Some(5), "{:?}", m.bands);
        assert!((m.bands[5] - -18.0).abs() < 0.5, "{:?}", m.bands);
        assert!(m.bands[0] < -60.0 && m.bands[9] < -60.0, "{:?}", m.bands);
        assert!((m.correlation - 1.0).abs() < 1e-6 && m.width < 1e-6);
    }

    #[test]
    fn loudness_matches_bs1770_for_a_1k_sine() {
        // A stereo 1 kHz sine at -18 dBFS RMS a channel: -18 + 3 (two
        // channels) - 0.691 + the K-filter's ~+0.69 at 1 kHz ≈ -15.0 LUFS.
        let s = sine(1_000.0, -18.0, 5.0);
        let m = measure(&s, &s, RATE, Bands::Octave);
        assert!((m.lufs - -14.99).abs() < 0.1, "{m:?}");
        assert!((m.short_max - m.lufs).abs() < 0.1, "{m:?}");
        // True peak of a sine: its amplitude, -15 dBFS.
        assert!((m.true_peak - -14.99).abs() < 0.2, "{m:?}");
    }

    /// The one-pass running sums are the two-pass K-weighting's, to the
    /// bit: a faster MIX_MEASURE reads the same LUFS (studio#685).
    #[test]
    fn one_pass_k_sums_are_the_two_pass_ones_exactly() {
        let l: Vec<f32> = sine(60.0, -12.0, 2.0)
            .iter()
            .zip(sine(3_000.0, -20.0, 2.0))
            .map(|(a, b)| a + b)
            .collect();
        let r: Vec<f32> = sine(440.0, -9.0, 2.0)[..90_000].to_vec();
        let n = l.len().min(r.len());
        let rate = f64::from(RATE);
        let (kl, kr) = (k_weighted(&l[..n], rate), k_weighted(&r[..n], rate));
        let mut want = vec![0.0];
        let mut sum = 0.0;
        for i in 0..n {
            sum += kl[i] * kl[i] + kr[i] * kr[i];
            want.push(sum);
        }
        let got = k_sums(&l, &r, rate);
        assert_eq!(got.len(), want.len());
        assert!(
            got.iter()
                .zip(&want)
                .all(|(a, b)| a.to_bits() == b.to_bits()),
            "the running sums moved"
        );
    }

    #[test]
    fn third_octave_bands_find_a_low_tone() {
        let s = sine(63.0, -24.0, 3.0);
        let m = measure(&s, &s, RATE, Bands::Third);
        let centres = Bands::Third.centres();
        let loudest = m
            .bands
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| centres[i])
            .unwrap_or_default();
        assert!((loudest - 63.0).abs() < 3.0, "{loudest} {:?}", m.bands);
    }

    #[test]
    fn silence_reads_the_floor_and_opposite_channels_anticorrelate() {
        let z = vec![0.0f32; 48_000];
        let m = measure(&z, &z, RATE, Bands::Octave);
        assert_eq!(m.lufs, FLOOR);
        assert_eq!(m.rms, FLOOR);
        let s = sine(500.0, -20.0, 1.0);
        let neg: Vec<f32> = s.iter().map(|x| -x).collect();
        let m = measure(&s, &neg, RATE, Bands::Octave);
        assert!((m.correlation + 1.0).abs() < 1e-6);
    }
}
