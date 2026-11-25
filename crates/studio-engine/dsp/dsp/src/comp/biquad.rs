//! Butterworth crossover/sidechain filter designers.
//!
//! The filters themselves are `dsp_core::audio::biquad::Biquad` (TDF2,
//! denormal-flushed); this module only keeps the comp-specific 2nd-order
//! Butterworth bilinear-transform designers used for multiband crossover
//! and sidechain EQ.
//!
//! Difference equation realized by the shared biquad:
//!
//! ```text
//! y[n] = b0*x[n] + b1*x[n-1] + b2*x[n-2] - a1*y[n-1] - a2*y[n-2]
//! ```

use std::f64::consts::PI;

pub use dsp_core::audio::biquad::Biquad;

/// Compute low-pass biquad filter coefficients from cutoff frequency.
///
/// Uses bilinear transform to convert analog Butterworth pole to digital
/// biquad. `normalized_cutoff` is 0.0..1.0 where 1.0 = Nyquist.
#[must_use]
pub fn design_lowpass_biquad(normalized_cutoff: f64) -> Biquad {
    let fc = normalized_cutoff.clamp(0.001, 0.999);
    let tan_half_omega = (PI * fc / 2.0).tan();
    let a = 1.0 / (2.0f64.mul_add(tan_half_omega, 1.0) + tan_half_omega * tan_half_omega);

    let mut filter = Biquad::new();
    filter.b0 = a * tan_half_omega * tan_half_omega;
    filter.b1 = 2.0 * a * tan_half_omega * tan_half_omega;
    filter.b2 = a * tan_half_omega * tan_half_omega;
    filter.a1 = 2.0 * a * (tan_half_omega * tan_half_omega - 1.0);
    filter.a2 = a * (2.0f64.mul_add(-tan_half_omega, 1.0) + tan_half_omega * tan_half_omega);
    filter
}

/// Compute high-pass biquad filter coefficients from cutoff frequency.
///
/// Butterworth high-pass counterpart of [`design_lowpass_biquad`].
#[must_use]
pub fn design_highpass_biquad(normalized_cutoff: f64) -> Biquad {
    let fc = normalized_cutoff.clamp(0.001, 0.999);
    let tan_half_omega = (PI * fc / 2.0).tan();
    let a = 1.0 / (2.0f64.mul_add(tan_half_omega, 1.0) + tan_half_omega * tan_half_omega);

    let mut filter = Biquad::new();
    filter.b0 = a;
    filter.b1 = -2.0 * a;
    filter.b2 = a;
    filter.a1 = 2.0 * a * (tan_half_omega * tan_half_omega - 1.0);
    filter.a2 = a * (2.0f64.mul_add(-tan_half_omega, 1.0) + tan_half_omega * tan_half_omega);
    filter
}
