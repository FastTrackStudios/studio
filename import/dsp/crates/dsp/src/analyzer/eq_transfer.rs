//! Measuring how far apart two EQs are, on the terms the Pro-Q work used.
//!
//! This is the measurement every number in the Pro-Q matching effort came
//! from, lifted out of the `eq_match` example so that it has exactly one
//! implementation. A second copy would be worse than no measurement at all:
//! the whole method rests on comparisons between runs — a change is accepted
//! or rejected by whether the library's error moved — and two copies drifting
//! apart would make those comparisons quietly meaningless.
//!
//! The method, and why each part of it is the way it is:
//!
//! - **Decorrelated stereo noise.** Mid and side are independent, so a band
//!   placed on either one has something to work on. A mono stimulus measures
//!   a Side band as silence.
//! - **Mid and side compared separately.** A stereo-placed band shows up in
//!   one and not the other, and averaging the two channels first would hide
//!   exactly the errors worth finding.
//! - **Settle, then measure.** Auto-threshold bands walk for something like
//!   seven seconds. Reading them before that measures the transient, and the
//!   same configuration measured twice disagreed by 7 dB depending on what
//!   had been rendered before it.
//! - **Third-octave bands, energy ratio.** Bin-by-bin comparison of two
//!   engines fed noise measures the noise, not the engines.
//!
//! The library hosts no plugins — callers render the buffers and hand them
//! over, which is what lets this compile and test with nothing installed.

use realfft::RealFftPlanner;

/// Analysis frame. Long enough to resolve a narrow notch at the bottom end.
pub const FFT: usize = 8192;
/// Frames averaged into each spectrum, after the warmup.
pub const FRAMES: usize = 24;
/// Frames rendered and thrown away before anything is measured — a little
/// over eight seconds at 48 kHz, which is what an auto-threshold band needs
/// before it holds still.
pub const WARMUP_FRAMES: usize = 95;

/// How the stimulus is coloured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stimulus {
    /// Flat noise, independent in mid and side.
    #[default]
    Flat,
    /// The same bed with six resonances in it.
    ///
    /// Flat noise is a degenerate input for anything that reacts to spectrum:
    /// a resonance suppressor has nothing to suppress, and what little it does
    /// is driven by the noise's own bin-to-bin fluctuation, which two engines
    /// will never agree on in detail. Programme material has peaks.
    Tonal,
    /// One noise source in both channels — no side content at all. Kept
    /// because it is the honest way to measure a mid-only chain.
    Mono,
}

/// Deterministic noise. A fixed spectrum across runs keeps comparisons
/// stable, and both engines must hear the identical signal.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        ((self.0 >> 33) as f64 / (1u64 << 31) as f64) - 1.0
    }
}

/// The six partials `Stimulus::Tonal` adds, spread across the spectrum.
const PARTIALS: [f64; 6] = [110.0, 275.0, 700.0, 1650.0, 3900.0, 9200.0];
/// Each partial's amplitude, relative to the noise bed's.
const PARTIAL_GAIN: f64 = 1.6;

/// How many frames a full measurement needs at this FFT size.
#[must_use]
pub fn frames_needed() -> usize {
    (WARMUP_FRAMES + FRAMES + 2) * FFT / 2 + FFT
}

/// Build the test signal: `(left, right)`.
#[must_use]
pub fn stimulus(
    frames: usize,
    amplitude: f64,
    kind: Stimulus,
    sample_rate: f64,
) -> (Vec<f32>, Vec<f32>) {
    let tonal = kind == Stimulus::Tonal;
    let mono = kind == Stimulus::Mono;
    let mut rng = Lcg(0xC0FF_EE01);
    let mut side_rng = Lcg(0x5EED_1D0F);
    // The side carries the SAME resonances, a third of a turn out of phase.
    // Put them in the mid alone and the side's spectrum there is noise 36 dB
    // down inside the band being read, so the side transfer function measures
    // whatever asymmetry leaks out of either engine rather than anything
    // either engine is doing.
    let mut phase = [0.0f64; 6];
    let mut side_phase = [2.1f64; 6];
    // Normalise so a tonal run sits at the SAME level as a flat one — one
    // variable per probe. Six partials at 1.6x raise the RMS by 13.8 dB,
    // which would otherwise measure how each engine behaves driven hard.
    let scale = if tonal {
        // variance = a²/3 (noise) + 6·(1.6a)²/2 (partials)
        ((1.0 / 3.0) / (1.0 / 3.0 + 6.0 * PARTIAL_GAIN * PARTIAL_GAIN / 2.0)).sqrt()
    } else {
        1.0
    };
    let amplitude = amplitude * scale;

    let (mut left, mut right) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
    for _ in 0..frames {
        let mut mid = amplitude * rng.next();
        if tonal {
            for (k, f) in PARTIALS.iter().enumerate() {
                mid += amplitude * PARTIAL_GAIN * phase[k].sin();
                phase[k] += std::f64::consts::TAU * f / sample_rate;
            }
        }
        let mut side = if mono {
            0.0
        } else {
            amplitude * side_rng.next()
        };
        if tonal && !mono {
            for (k, f) in PARTIALS.iter().enumerate() {
                side += amplitude * PARTIAL_GAIN * side_phase[k].sin();
                side_phase[k] += std::f64::consts::TAU * f / sample_rate;
            }
        }
        left.push((mid + side) as f32);
        right.push((mid - side) as f32);
    }
    (left, right)
}

/// Mid and side of a channel pair.
#[must_use]
pub fn to_ms(left: &[f32], right: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let mid = left.iter().zip(right).map(|(l, r)| 0.5 * (l + r)).collect();
    let side = left.iter().zip(right).map(|(l, r)| 0.5 * (l - r)).collect();
    (mid, side)
}

/// Average magnitude spectrum, in linear units, skipping the warmup.
///
/// # Panics
///
/// Panics if the FFT processing fails (which should not happen with valid input).
#[must_use]
pub fn spectrum(buf: &[f32]) -> Vec<f64> {
    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(FFT);
    let mut mag = vec![0.0f64; FFT / 2 + 1];
    let mut used = 0usize;

    // Hann, so leakage does not smear a narrow notch across its neighbours.
    let window: Vec<f64> = (0..FFT)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / FFT as f64).cos())
        .collect();

    let mut pos = WARMUP_FRAMES * FFT / 2;
    while pos + FFT <= buf.len() && used < FRAMES {
        let mut frame: Vec<f64> = (0..FFT).map(|i| buf[pos + i] as f64 * window[i]).collect();
        let mut out = fft.make_output_vec();
        fft.process(&mut frame, &mut out).expect("fft");
        for (m, c) in mag.iter_mut().zip(out.iter()) {
            *m += c.norm();
        }
        used += 1;
        pos += FFT / 2; // 50% overlap
    }
    for m in &mut mag {
        *m /= used.max(1) as f64;
    }
    mag
}

/// Third-octave comparison centres, 25 Hz to 16 kHz.
#[must_use]
pub fn band_centres() -> Vec<f64> {
    let mut f = 25.0f64;
    let mut out = Vec::new();
    while f <= 16_000.0 {
        out.push(f);
        f *= 2.0f64.powf(1.0 / 3.0);
    }
    out
}

/// Transfer function in dB at each centre, as an energy ratio over the band.
#[must_use]
pub fn response_db(input: &[f64], output: &[f64], centres: &[f64], sample_rate: f64) -> Vec<f64> {
    let bin_hz = sample_rate / FFT as f64;
    let sixth = 2.0f64.powf(1.0 / 6.0);
    centres
        .iter()
        .map(|&c| {
            let (lo, hi) = (c / sixth, c * sixth);
            let (mut num, mut den) = (0.0f64, 0.0f64);
            for i in 0..input.len().min(output.len()) {
                let f = i as f64 * bin_hz;
                if f >= lo && f <= hi {
                    num += output[i] * output[i];
                    den += input[i] * input[i];
                }
            }
            if den <= 1e-30 {
                0.0
            } else {
                10.0 * (num / den).log10()
            }
        })
        .collect()
}

/// How far apart two renders of the same stimulus are.
#[derive(Debug, Clone, Default)]
pub struct Difference {
    /// Mean absolute difference across every band of both components, in dB.
    pub mean_db: f64,
    /// The largest single-band difference.
    pub worst_db: f64,
    /// Where that was.
    pub worst_hz: f64,
    /// True when the worst band was in the side rather than the mid.
    pub worst_in_side: bool,
}

/// Compare two engines' output on one stimulus.
///
/// `dry` is the signal both were fed; `a` and `b` are what they returned.
/// Mid and side are measured separately and pooled, which is what makes a
/// stereo-placed band visible.
#[must_use]
pub fn difference(
    dry: (&[f32], &[f32]),
    a: (&[f32], &[f32]),
    b: (&[f32], &[f32]),
    sample_rate: f64,
) -> Difference {
    let centres = band_centres();
    let mut out = Difference::default();
    let mut total = 0.0f64;
    let mut n = 0usize;

    let split = |p: (&[f32], &[f32])| {
        let (m, s) = to_ms(p.0, p.1);
        (spectrum(&m), spectrum(&s))
    };
    let (dry_m, dry_s) = split(dry);
    let (a_m, a_s) = split(a);
    let (b_m, b_s) = split(b);

    for (in_side, (dry, (ra, rb))) in [
        (false, (&dry_m, (&a_m, &b_m))),
        (true, (&dry_s, (&a_s, &b_s))),
    ] {
        let ra = response_db(dry, ra, &centres, sample_rate);
        let rb = response_db(dry, rb, &centres, sample_rate);
        for (i, c) in centres.iter().enumerate() {
            let d = (ra[i] - rb[i]).abs();
            total += d;
            n += 1;
            if d > out.worst_db {
                out.worst_db = d;
                out.worst_hz = *c;
                out.worst_in_side = in_side;
            }
        }
    }
    out.mean_db = total / n.max(1) as f64;
    out
}
