//! Frequency response computation (dual-path: biquad and ZPK).
//!
//! Provides magnitude response, phase response, and group delay evaluation
//! for cascaded biquad sections. Used for display rendering and
//! anti-cramping delay computation.

use std::f64::consts::PI;

use crate::eq::design::biquad::{Coeffs, eval_sos, mag_db_sos};
use crate::eq::math::zpk::Zpk;

/// Compute magnitude response in dB at the given frequencies.
///
/// Evaluates the cascade of biquad sections at each frequency and returns
/// the magnitude in decibels.
///
/// # Arguments
/// * `sections` - Cascade of biquad coefficient arrays
/// * `frequencies` - Frequencies in Hz to evaluate at
/// * `sample_rate` - Sample rate in Hz
#[must_use]
pub fn compute_magnitude_response(
    sections: &[Coeffs],
    frequencies: &[f64],
    sample_rate: f64,
) -> Vec<f64> {
    frequencies
        .iter()
        .map(|&freq| {
            let w = 2.0 * PI * freq / sample_rate;
            mag_db_sos(sections, w)
        })
        .collect()
}

/// Compute phase response in radians at the given frequencies.
///
/// Evaluates the cascade of biquad sections at each frequency and returns
/// the phase angle in radians (range: -pi to pi).
///
/// # Arguments
/// * `sections` - Cascade of biquad coefficient arrays
/// * `frequencies` - Frequencies in Hz to evaluate at
/// * `sample_rate` - Sample rate in Hz
#[must_use]
pub fn compute_phase_response(
    sections: &[Coeffs],
    frequencies: &[f64],
    sample_rate: f64,
) -> Vec<f64> {
    frequencies
        .iter()
        .map(|&freq| {
            let w = 2.0 * PI * freq / sample_rate;
            let h = eval_sos(sections, w);
            h.arg()
        })
        .collect()
}

/// Compute group delay in samples at a given frequency.
///
/// Group delay is the negative derivative of the phase response:
///   tau(w) = -d(phase)/dw
///
/// Computed using central finite differences for numerical stability.
///
/// # Arguments
/// * `sections` - Cascade of biquad coefficient arrays
/// * `freq_hz` - Frequency in Hz to evaluate at
/// * `sample_rate` - Sample rate in Hz
#[must_use]
pub fn compute_group_delay(sections: &[Coeffs], freq_hz: f64, sample_rate: f64) -> f64 {
    let w = 2.0 * PI * freq_hz / sample_rate;

    // Central difference step size. Small enough for accuracy,
    // large enough to avoid floating-point noise.
    let dw = 1e-6;

    let w_lo = (w - dw).max(1e-10);
    let w_hi = (w + dw).min(PI - 1e-10);
    let actual_dw = w_hi - w_lo;

    if actual_dw < 1e-15 {
        return 0.0;
    }

    let phase_lo = eval_sos(sections, w_lo).arg();
    let phase_hi = eval_sos(sections, w_hi).arg();

    // Unwrap phase difference: handle wrapping around +/-pi
    let mut dphi = phase_hi - phase_lo;
    for _ in 0..2 {
        if dphi <= PI {
            break;
        }
        dphi -= 2.0 * PI;
    }
    for _ in 0..2 {
        if dphi >= -PI {
            break;
        }
        dphi += 2.0 * PI;
    }

    // Group delay = -d(phase)/dw
    -dphi / actual_dw
}

/// Compute magnitude response from a ZPK representation.
///
/// Directly evaluates the ZPK transfer function without converting
/// to biquad sections. Useful for display before coefficient computation.
#[must_use]
pub fn compute_magnitude_response_zpk(
    zpk: &Zpk,
    frequencies: &[f64],
    sample_rate: f64,
) -> Vec<f64> {
    frequencies
        .iter()
        .map(|&freq| {
            let w = 2.0 * PI * freq / sample_rate;
            zpk.mag_db(w)
        })
        .collect()
}

/// Compute phase response from a ZPK representation.
#[must_use]
pub fn compute_phase_response_zpk(zpk: &Zpk, frequencies: &[f64], sample_rate: f64) -> Vec<f64> {
    frequencies
        .iter()
        .map(|&freq| {
            let w = 2.0 * PI * freq / sample_rate;
            zpk.eval_z(w).arg()
        })
        .collect()
}
