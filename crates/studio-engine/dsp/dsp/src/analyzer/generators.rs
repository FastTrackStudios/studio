//! Deterministic test signals.
//!
//! Every generator is a pure function of its arguments — no clocks, no RNG
//! seeded from the environment — so a comparison run is reproducible and can
//! be asserted on in CI.

use std::f64::consts::TAU;

/// A single-sample impulse: the excitation for decay and impulse-response
/// measurement.
#[must_use]
pub fn impulse(length: usize) -> Vec<f32> {
    let mut buf = vec![0.0f32; length];
    if let Some(first) = buf.first_mut() {
        *first = 1.0;
    }
    buf
}

/// A sine at `freq_hz`, amplitude 1.0.
#[must_use]
pub fn sine(freq_hz: f64, sample_rate: f64, length: usize) -> Vec<f32> {
    (0..length)
        .map(|i| (TAU * freq_hz * i as f64 / sample_rate).sin() as f32)
        .collect()
}

/// A sine at a given amplitude in dBFS.
#[must_use]
pub fn sine_db(freq_hz: f64, sample_rate: f64, length: usize, db: f64) -> Vec<f32> {
    let gain = 10.0f64.powf(db / 20.0) as f32;
    sine(freq_hz, sample_rate, length)
        .into_iter()
        .map(|s| s * gain)
        .collect()
}

/// An exponential (log) sine sweep from `start_hz` to `end_hz`.
///
/// Exponential rather than linear because it spends equal time per octave,
/// which is what a frequency-response comparison wants — and because its
/// harmonic distortion products separate cleanly in the deconvolved response.
#[must_use]
pub fn sweep(start_hz: f64, end_hz: f64, sample_rate: f64, length: usize) -> Vec<f32> {
    if length == 0 || start_hz <= 0.0 || end_hz <= 0.0 {
        return vec![0.0; length];
    }
    let duration = length as f64 / sample_rate;
    let ratio = end_hz / start_hz;
    let k = duration / ratio.ln();
    (0..length)
        .map(|i| {
            let t = i as f64 / sample_rate;
            let phase = TAU * start_hz * k * ((t / k).exp() - 1.0);
            phase.sin() as f32
        })
        .collect()
}

/// Reproducible white noise in `-1..1`, from a fixed seed.
///
/// xorshift64 rather than a real RNG so the sequence is identical on every
/// platform and every run — a comparison threshold is only meaningful if the
/// stimulus is bit-identical between the reference and the candidate.
#[must_use]
pub fn white_noise(length: usize, seed: u64) -> Vec<f32> {
    let mut state = if seed == 0 {
        0x9E37_79B9_7F4A_7C15
    } else {
        seed
    };
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            // Take the top 32 bits — the low bits of xorshift are weaker.
            let v = (state >> 32) as u32;
            (v as f64 / u32::MAX as f64 * 2.0 - 1.0) as f32
        })
        .collect()
}

/// Silence — used to measure a reverb tail after the excitation stops.
#[must_use]
pub fn silence(length: usize) -> Vec<f32> {
    vec![0.0; length]
}

/// An impulse followed by silence long enough to capture the whole tail.
///
/// This is the standard reverb stimulus: `tail_seconds` should exceed the
/// longest RT60 being measured, or the decay fit runs off the end of the
/// buffer.
#[must_use]
pub fn impulse_with_tail(tail_seconds: f64, sample_rate: f64) -> Vec<f32> {
    let n = (tail_seconds * sample_rate).max(1.0) as usize;
    impulse(n)
}
