//! Deterministic test signals.
//!
//! Every generator here is a pure function of its arguments — no clock, no
//! `rand`, no platform entropy — because a golden master is worthless if the
//! input drifts. The PRNG is a fixed xorshift written out in full for the same
//! reason: a dependency bump must not be able to invalidate every reference
//! vector in the tree.
//!
//! The periodic generators also compute in `f64` and round to `f32` once, at
//! the end. Platform libms disagree in the last bit of `sinf`/`expf`, and a
//! sweep's phase reaches tens of thousands of radians, where one `f32` ULP of
//! phase is a thousandth of a radian: computed in `f32`, the *excitation*
//! itself differed between x86_64 Linux and aarch64 macOS by up to -90 dB,
//! before any DSP ran. In `f64` the libms still disagree in the last bit, but
//! that bit is lost in the final rounding, so every platform feeds the
//! fixtures the same samples.

use crate::num::{count_to_f64, f64_to_f32, u32_to_f32};

/// A unit impulse at sample 0, silence after. Reveals a filter's entire
/// linear behaviour in one pass.
#[must_use]
pub fn impulse(len: usize) -> Vec<f32> {
    let mut v = vec![0.0; len];
    if let Some(first) = v.first_mut() {
        *first = 1.0;
    }
    v
}

/// A steady sine at `freq` Hz. Phase starts at zero.
#[must_use]
pub fn sine(len: usize, freq: f32, sample_rate: f32) -> Vec<f32> {
    let step = core::f64::consts::TAU * f64::from(freq) / f64::from(sample_rate);
    (0..len)
        .map(|n| f64_to_f32((count_to_f64(n) * step).sin()))
        .collect()
}

/// An exponential sweep from `f0` to `f1` — the standard excitation for
/// catching frequency-dependent breakage (a filter that only drifts at the top
/// octave shows up here and nowhere else).
#[must_use]
pub fn log_sweep(len: usize, f0: f32, f1: f32, sample_rate: f32) -> Vec<f32> {
    let n = count_to_f64(len).max(1.0);
    let (f0, f1, sample_rate) = (f64::from(f0), f64::from(f1), f64::from(sample_rate));
    let k = (f1 / f0).max(f64::MIN_POSITIVE).ln();
    (0..len)
        .map(|i| {
            let t = count_to_f64(i) / n;
            let phase = core::f64::consts::TAU * f0 * n / sample_rate / k * (t * k).exp_m1();
            f64_to_f32(phase.sin())
        })
        .collect()
}

/// White noise in `-1..1` from a fixed xorshift64* stream.
#[must_use]
pub fn noise(len: usize, seed: u64) -> Vec<f32> {
    let mut state = if seed == 0 {
        0x2545_F491_4F6C_DD1D
    } else {
        seed
    };
    (0..len)
        .map(|_| {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            let bits = u32::try_from(state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32).unwrap_or(0);
            u32_to_f32(bits >> 8).mul_add(2.0 / 16_777_216.0, -1.0)
        })
        .collect()
}

/// Decaying transient bursts on a fixed grid — the signal that exercises
/// envelope followers, gates and transient detectors, which a steady sine
/// leaves entirely idle.
#[must_use]
pub fn transients(len: usize, sample_rate: f32) -> Vec<f32> {
    let period = crate::num::f32_to_index(sample_rate * 0.25).max(1);
    let rate = f64::from(sample_rate);
    let decay = (rate * 0.02).max(1.0);
    (0..len)
        .map(|n| {
            let since = n.checked_rem(period).unwrap_or(0);
            let env = (-count_to_f64(since) / decay).exp();
            let tone = (count_to_f64(n) * core::f64::consts::TAU * 180.0 / rate).sin();
            f64_to_f32(env * tone)
        })
        .collect()
}

/// The same excitation at double width, for cores that compute in `f64`.
///
/// Widening is exact, so an `f32` and an `f64` core fed through here see the
/// identical signal and their reference vectors stay comparable.
#[must_use]
pub fn widen(samples: &[f32]) -> Vec<f64> {
    samples.iter().copied().map(f64::from).collect()
}

/// Silence — the input that catches a stage humming, self-oscillating, or
/// leaking denormals when nothing is playing.
#[must_use]
pub fn silence(len: usize) -> Vec<f32> {
    vec![0.0; len]
}

/// Full-scale DC. A saturator's asymmetry and any DC-blocking stage both show
/// up immediately.
#[must_use]
pub fn dc(len: usize, level: f32) -> Vec<f32> {
    vec![level; len]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::num::count_to_f32;

    #[test]
    fn generators_are_reproducible() {
        assert_eq!(noise(256, 7), noise(256, 7));
        assert_eq!(
            log_sweep(256, 20.0, 20_000.0, 48_000.0),
            log_sweep(256, 20.0, 20_000.0, 48_000.0)
        );
    }

    #[test]
    fn generators_stay_finite_and_bounded() {
        let sr = 48_000.0;
        for buf in [
            impulse(1024),
            sine(1024, 440.0, sr),
            log_sweep(1024, 20.0, 20_000.0, sr),
            noise(1024, 1),
            transients(1024, sr),
        ] {
            assert!(buf.iter().all(|s| s.is_finite() && s.abs() <= 1.0001));
        }
    }

    #[test]
    fn noise_is_actually_centred() {
        let buf = noise(1 << 16, 42);
        let mean: f32 = buf.iter().sum::<f32>() / count_to_f32(buf.len());
        assert!(mean.abs() < 0.01, "noise mean drifted: {mean}");
    }
}
