//! Biquad filters the metrics need: octave-band splitting for per-band decay,
//! and the ITU-R BS.1770 K-weighting pair for loudness.
//!
//! These are measurement filters, not audio-path DSP — they run offline over a
//! captured buffer, so clarity wins over the hot-path constraints that govern
//! `features/fx/*-dsp`.

use std::f64::consts::PI;

/// A transposed-direct-form-II biquad section.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Biquad {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Biquad {
    /// Run the filter over a whole buffer, starting from rest.
    #[must_use]
    pub fn apply(&self, x: &[f32]) -> Vec<f32> {
        let (mut z1, mut z2) = (0.0f64, 0.0f64);
        x.iter()
            .map(|&s| {
                let s = s as f64;
                let y = self.b0 * s + z1;
                z1 = self.b1 * s - self.a1 * y + z2;
                z2 = self.b2 * s - self.a2 * y;
                y as f32
            })
            .collect()
    }

    /// Normalize by `a0` — every design below builds unnormalized coefficients.
    fn normalized(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// Constant-Q bandpass (RBJ cookbook), unity gain at the centre.
    #[must_use]
    pub fn bandpass(centre_hz: f64, q: f64, sample_rate: f64) -> Self {
        let w0 = 2.0 * PI * centre_hz / sample_rate;
        let (sin, cos) = (w0.sin(), w0.cos());
        let alpha = sin / (2.0 * q);
        Self::normalized(alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cos, 1.0 - alpha)
    }

    /// High-shelf (RBJ cookbook), `gain_db` above the corner.
    #[must_use]
    pub fn high_shelf(f0: f64, q: f64, gain_db: f64, sample_rate: f64) -> Self {
        let a = 10.0f64.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0 / sample_rate;
        let (sin, cos) = (w0.sin(), w0.cos());
        let alpha = sin / (2.0 * q);
        let sqrt_a = a.sqrt();
        Self::normalized(
            a * ((a + 1.0) + (a - 1.0) * cos + 2.0 * sqrt_a * alpha),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
            a * ((a + 1.0) + (a - 1.0) * cos - 2.0 * sqrt_a * alpha),
            (a + 1.0) - (a - 1.0) * cos + 2.0 * sqrt_a * alpha,
            2.0 * ((a - 1.0) - (a + 1.0) * cos),
            (a + 1.0) - (a - 1.0) * cos - 2.0 * sqrt_a * alpha,
        )
    }

    /// Second-order highpass (RBJ cookbook).
    #[must_use]
    pub fn highpass(f0: f64, q: f64, sample_rate: f64) -> Self {
        let w0 = 2.0 * PI * f0 / sample_rate;
        let (sin, cos) = (w0.sin(), w0.cos());
        let alpha = sin / (2.0 * q);
        Self::normalized(
            (1.0 + cos) / 2.0,
            -(1.0 + cos),
            (1.0 + cos) / 2.0,
            1.0 + alpha,
            -2.0 * cos,
            1.0 - alpha,
        )
    }
}

/// The octave-band centre frequencies decay is reported over.
///
/// Stops at 8 kHz: the next band up (16 kHz) is above Nyquist for a 32 kHz
/// project and carries almost no reverb energy anyway.
pub const OCTAVE_CENTRES_HZ: [f64; 8] = [62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0];

/// Q for a one-octave-wide constant-Q bandpass section.
const OCTAVE_Q: f64 = 1.414;

/// Bandpass sections cascaded per band.
///
/// A constant-Q bandpass has only 6 dB/octave skirts, which is nowhere near
/// enough isolation for decay measurement: a reverb's energy is concentrated
/// in the low-mids, and when those decay faster than the top (a tamed low
/// end, say) their leakage into a high band decays with them and drags that
/// band's fitted RT60 down. A 300 Hz low-shelf cut appeared to halve the
/// 4 kHz band's decay — that was leakage, not the reverb.
///
/// Three sections give 18 dB/octave, so a tone four octaves away lands ~72 dB
/// down and cannot influence the fit.
const OCTAVE_SECTIONS: usize = 3;

/// Split a buffer into octave bands, in [`OCTAVE_CENTRES_HZ`] order.
///
/// Bands whose centre is at or above Nyquist are returned as silence rather
/// than being dropped, so the band index always lines up with the frequency
/// table regardless of sample rate.
#[must_use]
pub fn octave_bands(x: &[f32], sample_rate: f64) -> Vec<Vec<f32>> {
    OCTAVE_CENTRES_HZ
        .iter()
        .map(|&c| {
            if c >= sample_rate / 2.0 {
                return vec![0.0; x.len()];
            }
            let bp = Biquad::bandpass(c, OCTAVE_Q, sample_rate);
            let mut out = bp.apply(x);
            for _ in 1..OCTAVE_SECTIONS {
                out = bp.apply(&out);
            }
            out
        })
        .collect()
}
