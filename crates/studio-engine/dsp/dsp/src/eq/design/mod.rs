//! Top-level filter design dispatcher matching Pro-Q 4's
//! `design_filter_zpk_and_transform` (0x1800ff6f0).
//!
//! Maps filter type + parameters to a vector of biquad sections by dispatching
//! to the appropriate design path:
//!   - LP/HP: Butterworth prototype -> bilinear -> Q adjustment
//!   - BP: Butterworth LP -> LP->BP transform -> bilinear -> normalize
//!   - Notch: Butterworth LP -> LP->BS transform -> bilinear -> normalize
//!   - Peak: `cascade::compute_cascade_peak`
//!   - Shelves: shelf module functions
//!   - Allpass: Butterworth -> bilinear -> reflect zeros
//!   - `ShelfAlt`: `cascade::compute_cascade_shelf_alt`

use crate::eq::inline::{InlineVec, inline_vec};

use dsp_core::num;
use std::f64::consts::PI;

use biquad::Coeffs;

mod allpass;
mod bandpass;
mod bell;
pub mod biquad;
mod brickwall;
pub mod cascade;
mod common;
pub mod constants;
pub mod fractional;
mod hp;
mod lp;
pub mod mzt;
mod notch;
pub mod per_section;
mod shelf;
pub mod shelf_proq4;
pub mod shelf_zpk;
pub mod slope;
mod tilt;
use allpass::{design_allpass_with_lookup, design_bandpass_variant};
use bandpass::mzt_bandpass_simple_cascade;
use bell::mzt_peak_cascade;
use hp::mzt_highpass_simple_cascade;
use lp::mzt_lowpass_simple_cascade;
use notch::mzt_notch_simple_cascade;
use shelf::shelf_universal_synth_cascade;
use tilt::mzt_tilt_shelf_cascade;

/// Filter types matching Pro-Q 4's type codes (0-12).
///
/// From `filter_type_dispatcher` (0x1800fe2a0) and `apply_eq_band_parameters_full` (0x1801110b0):
///   0 = Peak/Bell, 1 = HP, 2 = LP, 3 = BP, 4 = Notch,
///   5 = Band Pass variant, 6 = Flat Tilt,
///   7 = Low Shelf, 8 = High Shelf, 9 = Tilt Shelf,
///   10 = Band Shelf, 11 = Allpass, 12 = Shelf Alt
///
/// Type 6 (Flat Tilt) identified from binary: `apply_eq_band_parameters_full` uses
/// `cos(Q) * pow(const, cos(Q)*scale + offset)` frequency mapping for type 6,
/// and `apply_shelf_gain_to_zpk` squares the gain for type 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterType {
    Peak,            // type 0 — own ZPK via compute_cascade_coefficients
    Highpass,        // type 1 — Butterworth direct
    Lowpass,         // type 2 — Butterworth direct
    Bandpass,        // type 3 — Butterworth LP + elliptic LP→BP
    Notch,           // type 4 — Butterworth LP + LP→BS
    BandPassVariant, // type 5 — elliptic LP→BP with special Q mapping
    FlatTilt,        // type 6 — cos-based frequency mapping + LP→BP + gain²
    LowShelf,        // type 7 — Butterworth + bilinear + shelf gain
    HighShelf,       // type 8 — Butterworth + bilinear + shelf gain
    TiltShelf,       // type 9 — Butterworth + bilinear + shelf gain
    BandShelf,       // type 10 — LP→BP + bilinear
    Allpass,         // type 11 — negate zeros (transform type 4)
    ShelfAlt,        // type 12 — own ZPK via compute_cascade_coefficients
}

/// Design a complete filter and return biquad sections.
///
/// This is the main entry point, equivalent to `setup_eq_band_filter` +
/// `design_filter_zpk_and_transform`.
///
/// Parameters:
///   - `filter_type`: which filter shape
///   - `freq_hz`: center/corner frequency in Hz
///   - `q`: quality factor (bandwidth control)
///   - `gain_db`: gain in dB (for peak/shelf types)
///   - `sample_rate`: audio sample rate in Hz
///   - `order`: filter order (2, 4, 6, 8, ... -- number of poles)
///
/// Returns a vector of biquad coefficient arrays, one per section.
#[must_use]
pub fn design_filter(
    filter_type: FilterType,
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    order: usize,
) -> InlineVec<Coeffs> {
    let order = order.max(1);
    let n = order.div_ceil(2);

    let _ = common::slope_from_pole_count(order);

    match filter_type {
        FilterType::Lowpass => {
            if order == crate::eq::design::slope::BRICKWALL_ORDER {
                return brickwall::brickwall_cascade(freq_hz, sample_rate, false);
            }
            // Order 1 is 6 dB/oct, not bypass — `Slope::Db0` is order ZERO and
            // `Band::update` short-circuits that before it gets here. Returning
            // a pass-through for order 1 meant the shallowest cut Pro-Q offers
            // did nothing at all.
            if order == 1 {
                return inline_vec![fractional::first_order_cut(freq_hz, sample_rate, false)];
            }
            mzt_lowpass_simple_cascade(n, freq_hz, q, sample_rate, order)
        }
        FilterType::Highpass => {
            if order == crate::eq::design::slope::BRICKWALL_ORDER {
                return brickwall::brickwall_cascade(freq_hz, sample_rate, true);
            }
            if order == 1 {
                return inline_vec![fractional::first_order_cut(freq_hz, sample_rate, true)];
            }
            mzt_highpass_simple_cascade(n, freq_hz, q, sample_rate, order)
        }
        FilterType::Bandpass => {
            if order == 1 {
                return inline_vec![biquad::PASSTHROUGH];
            }
            mzt_bandpass_simple_cascade(n, freq_hz, q, sample_rate, order)
        }
        FilterType::Notch => mzt_notch_simple_cascade(n, freq_hz, q, sample_rate),
        FilterType::BandPassVariant => design_bandpass_variant(n, freq_hz, q, sample_rate),
        FilterType::Peak => mzt_peak_cascade(n, freq_hz, q, gain_db, sample_rate, order),
        FilterType::ShelfAlt => {
            cascade::compute_cascade_shelf_alt(freq_hz, q, gain_db, sample_rate, order)
        }
        FilterType::LowShelf => mzt_low_shelf_cascade(n, freq_hz, q, gain_db, sample_rate, order),
        FilterType::HighShelf => mzt_high_shelf_cascade(n, freq_hz, q, gain_db, sample_rate, order),
        FilterType::TiltShelf => mzt_tilt_shelf_cascade(n, freq_hz, q, gain_db, sample_rate, order),
        FilterType::BandShelf => {
            shelf_zpk::design_band_shelf_zpk(n, freq_hz, q, gain_db, sample_rate)
        }
        FilterType::Allpass => design_allpass_with_lookup(n, freq_hz, q, sample_rate, order),
        FilterType::FlatTilt => {
            cascade::compute_cascade_flat_tilt(freq_hz, q, gain_db, sample_rate, order)
        }
    }
}

// ── MZT-based cascaded designs ──────────────────────────────────────────────

/// Low shelf cascade — Pro-Q 4 Butterworth cascade with per-pole gain g = gain^(1/(2N)).
///
/// Analog prototype (per `docs/reports/proq4/re/shelf_cascade_higher_slopes.md`):
///   - Butterworth angles `θ_i` = π(2i+1)/(2N)
///   - Poles at `(-sin θ_i, ±cos θ_i) · wa / g`       (scaled INWARD in frequency)
///   - Zeros at `(-sin θ_i, ±cos θ_i) · wa · g`       (scaled OUTWARD in frequency)
///   - wa = 2·fs·tan(π·fc/fs) (pre-warped corner)
///
///   Each 2nd-order analog section is bilinear-transformed to a digital biquad.
fn mzt_low_shelf_cascade(
    n: usize,
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    pole_count: usize,
) -> InlineVec<Coeffs> {
    if gain_db.abs() < 1e-9 {
        return inline_vec![biquad::PASSTHROUGH; n.max(1)];
    }
    // Universal-synth folds in the per-section anti-cramping clamps
    // (W_POLE_MAX ≈ 2.99, W_THIRD_MAX ≈ 2.36, W_ZERO_MAX ≈ 1.88) decoded from
    // `compute_shelf_band_parameters` — these are what carry shelves to
    // 3744/3744 pure-algorithmic conformance.
    shelf_universal_synth_cascade(
        n,
        freq_hz,
        q,
        gain_db,
        sample_rate,
        pole_count,
        /*high_shelf=*/ false,
    )
}

fn mzt_high_shelf_cascade(
    n: usize,
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    pole_count: usize,
) -> InlineVec<Coeffs> {
    if gain_db.abs() < 1e-9 {
        return inline_vec![biquad::PASSTHROUGH; n.max(1)];
    }
    shelf_universal_synth_cascade(
        n,
        freq_hz,
        q,
        gain_db,
        sample_rate,
        pole_count,
        /*high_shelf=*/ true,
    )
}

/// Apply Gain-Q interaction to a peak filter.
///
/// From Pro-Q 4 binary (`compute_peak_band_parameters` at 0x18010de30):
/// The gain-Q interaction coefficient at offset 0x8c modifies Q:
///   `Q_effective = gain_q_coeff² * scaling_constant + base_Q`
///
/// When enabled, Q narrows as gain increases (analog console behavior).
/// The interaction amount (0.0-1.0) controls how much gain affects Q.
///
/// Only applies to Bell (Peak) filter type.
#[must_use]
pub fn apply_gain_q_interaction(q: f64, gain_db: f64, interaction: f64) -> f64 {
    if interaction.abs() < 0.001 {
        return q;
    }

    // From binary: the interaction coefficient is squared and scaled
    // gain_q_coeff² * DAT_1802319b8 + base_Q
    // DAT_1802319b8 is a scaling factor
    //
    // The effect: higher gain → narrower Q (higher Q value)
    // Clamped to reasonable range
    let gain_linear = gain_db.abs() / 30.0; // normalize gain to 0-1 range
    let q_shift = interaction * interaction * gain_linear * 0.5;

    // Q increases (narrows) with gain when interaction is positive
    let q_modified = q * (1.0 + q_shift);
    q_modified.clamp(0.025, 40.0)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "complex/float arithmetic — `Complex` is two `f64`s, so its operators cannot panic or overflow; the lint cannot see through an operator overload"
)]
/// Compute auto-gain compensation for current EQ settings.
///
/// From Pro-Q 4 binary: "`AutoGain`" parameter at 0x18022ccf8.
/// Manual states: "Pro-Q automatically compensates for increase or loss of gain
/// after `EQing`. The applied make-up gain is an educated guess based on the
/// current EQ settings, and is not a dynamic process."
///
/// Implementation: evaluate the combined EQ response at key frequency points
/// and compute the RMS level change, then invert it.
#[must_use]
pub fn compute_auto_gain(band_sections: &[InlineVec<Coeffs>], sample_rate: f64) -> f64 {
    use crate::eq::math::zpk::Complex;

    // Evaluate combined response at logarithmically-spaced frequencies
    // spanning the audible range (20 Hz - 20 kHz)
    let num_points = 64;
    let f_low = 20.0_f64;
    let f_high = 20000.0_f64;
    let log_range = (f_high / f_low).ln();

    let mut sum_db = 0.0;
    let mut count: usize = 0;

    for i in 0..num_points {
        let t = f64::from(i) / f64::from(num_points - 1);
        let freq = f_low * (t * log_range).exp();
        let w = 2.0 * PI * freq / sample_rate;

        // Evaluate combined response of all bands
        let mut h = Complex::ONE;
        for sections in band_sections {
            let ejw = Complex::from_polar(1.0, w);
            let ejw2 = ejw * ejw;
            for c in sections {
                let den = Complex::new(c[0], 0.0)
                    + ejw * Complex::new(c[1], 0.0)
                    + ejw2 * Complex::new(c[2], 0.0);
                let num = Complex::new(c[3], 0.0)
                    + ejw * Complex::new(c[4], 0.0)
                    + ejw2 * Complex::new(c[5], 0.0);
                if den.mag() > 1e-30 {
                    h = h * num / den;
                }
            }
        }

        let mag_db = 20.0 * h.mag().log10();
        if mag_db.is_finite() {
            sum_db += mag_db;
            count = count.saturating_add(1);
        }
    }

    if count > 0 {
        // Return negative of average gain change (compensation)
        -(sum_db / num::count_to_f64(count))
    } else {
        0.0
    }
}
