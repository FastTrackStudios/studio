//! Pro-Q 4 per-section sub-frequency helpers.
//!
//! Decompiled from Pro-Q 4 binary; see
//! `docs/reports/proq4/re/per_section_helpers_decompiled.md` for the source
//! decompilations and the canonical `proto` (0xC0-byte) layout.
//!
//! Pipeline: `prepare_band_display_info` runs `compute_zpk_transfer_function_coefficients`
//! and `solve_biquad_denominator_quadratic`, then dispatches on the per-section
//! internal filter type stored at `proto[0x13]` (= `proto+0x98`, mirrored from
//! `sec[+0x58]`):
//!
//!   | proto[0x13] | helper                                |
//!   |------------:|---------------------------------------|
//!   | 0, 3        | `compute_peak_type3_parameters`       |
//!   | 1, 2, 4, 5, 6 | `compute_notch_type46_parameters`   |
//!   | 7           | `compute_shelf_band_parameters`       |
//!   | 8           | `compute_band_shelf_parameters_v2`    |
//!   | 10          | `compute_band_shelf_parameters`       |
//!   | other       | inline (caller-side fallback)         |
//!
//! Each helper writes (`wp`, `wz`, `wt`, `w_eval`) into `proto`, and these
//! sub-frequencies feed the universal Lagrange-MZT synth at
//! `crates/eq-dsp/src/cascade.rs:1587`.
//!
//! Status: `peak_type3` (type=0 branch) ported and bit-exact verified by
//! construction. Other helpers staged with explicit `unimplemented!` until
//! their respective probe captures are wired in.

use crate::eq::design::biquad::Coeffs;
use crate::eq::design::cascade::proq4_s2_from_prototype_with_subfreq_pub;
use dsp_core::num;
use std::f64::consts::PI;

mod analog;
mod limits;
mod notch;
mod peak;
mod shelf;

// The shape helpers live in the files above. This module's surface is
// unchanged, so every existing `per_section::foo` call site still resolves
// and the split stays a pure move.
pub use analog::{
    AnalogBiquad, MagSqCoeffs, PoleRoots, Prototype, compute_zpk_transfer_coeffs_generic,
    eval_squared_mag_scalar, omega_scale_for_band_type, solve_biquad_denominator_quadratic_generic,
};
pub use limits::{check_frequency_within_band_limits, update_tracked_band_frequencies};
pub use notch::compute_notch_type46_parameters;
pub use peak::compute_peak_type3_parameters;
pub use shelf::{
    compute_band_shelf_parameters, compute_band_shelf_parameters_v2, compute_shelf_band_parameters,
};

/// Dispatch a `proto` through the helper that matches its `section_type`.
///
/// Mirrors the switch in `prepare_band_display_info` (Pro-Q 4 binary) that
/// chooses one of the 5 helpers based on `proto[0x13]` (= `sec[+0x58]`).
///
/// Returns `Err(SectionType)` for inline/fallback section types not handled
/// by a dedicated helper — caller is expected to apply the inline default
/// (typically `wz = wp·0.05`, `wt = wp·0.5`).
///
/// # Errors
///
/// Returns `Err(section_type)` when `proto.section_type` does not match
/// a handled type (0, 1, 2, 3, 4, 5, 6, 7, 8, or 10).
pub fn dispatch_section_helper(proto: &mut Prototype) -> Result<(), i32> {
    match proto.section_type {
        0 | 3 => {
            compute_peak_type3_parameters(proto);
            Ok(())
        }
        1 | 2 | 4 | 5 | 6 => {
            compute_notch_type46_parameters(proto);
            Ok(())
        }
        7 => {
            compute_shelf_band_parameters(proto);
            Ok(())
        }
        8 => {
            compute_band_shelf_parameters_v2(proto);
            Ok(())
        }
        10 => {
            compute_band_shelf_parameters(proto);
            Ok(())
        }
        other => Err(other),
    }
}

/// Universal section-synth entry point.
///
/// End-to-end pipeline that mirrors Pro-Q's `prepare_band_display_info` →
/// `compute_audio_biquad_lagrange_mzt`:
///
/// 1. The caller supplies `proto` already populated upstream by
///    `compute_zpk_transfer` + `solve_biquad` (so `wp`, `wz`, `mode`, and
///    `band_omega_ref` are valid going in).
/// 2. Dispatch into the per-section helper based on `proto.section_type`
///    (or apply inline defaults for unhandled types).
/// 3. Hand off `(wp, wz, wt, w_eval)` plus the analog ZPK to the
///    Lagrange-MZT synth at `cascade::proq4_s2_from_prototype_with_subfreq_pub`.
///
/// `omega_scale` is the per-band-type ω₀ scaling factor (use
/// [`omega_scale_for_band_type`]); `freq_hz` is the BAND fc (radians ω₀
/// inside the synth = `2π · freq_hz · omega_scale / sample_rate`).
///
/// Returns `(coeffs, fallback_type)` — `fallback_type` is `Some(t)` when
/// dispatch fell into the inline branch (no dedicated helper for `t`), so
/// the caller can log/iterate as needed.
pub fn proq4_universal_section_synth(
    proto: &mut Prototype,
    analog: &AnalogBiquad,
    freq_hz: f64,
    sample_rate: f64,
    omega_scale: f64,
) -> (Coeffs, Option<i32>) {
    // Step 1: refresh A..F from the analog ZPK at this section's ω.
    // Step 2: solve_biquad → seed (wp, wz) and mode for the helper.
    let omega = 2.0 * PI * freq_hz * omega_scale / sample_rate;
    let (mag_coeffs, is_quadratic) = compute_zpk_transfer_coeffs_generic(analog, omega);
    let roots = solve_biquad_denominator_quadratic_generic(&mag_coeffs, is_quadratic);
    proto.mode = i32::from(roots.count);
    proto.root_count_dup = i32::from(roots.count);
    if roots.count >= 1 {
        proto.wp = roots.w1;
    }
    if roots.count >= 2 {
        proto.wz = roots.w2;
    }
    if matches!(proto.section_type, 2 | 5) && roots.count >= 2 {
        let aux = (roots.w2.min(PI) / PI - 0.8).clamp(0.0, 0.2);
        proto.alpha_scratch_8c = num::narrow(aux * aux * 25.0);
    }
    // Synchronize derived caches the helpers may read.
    proto.analog = Some(*analog);
    proto.omega_band = omega;
    proto.stored_e = f64::from_bits(0x4008_a14d_57b3_73df);
    proto.stored_f = f64::from_bits(0x4006_9e95_6570_8efc);
    proto.stored_g = f64::from_bits(0x4008_81c6_8e4d_6f74);

    // Step 3: dispatch into the per-section helper (or apply inline default).
    let fallback = match dispatch_section_helper(proto) {
        Ok(()) => None,
        Err(t) => {
            apply_inline_section_defaults(proto);
            Some(t)
        }
    };
    proto.wp = proto.wp.min(proto.stored_e);
    proto.wz = proto.wz.min(proto.stored_f);
    proto.wt = proto.wt.min(proto.stored_g);

    // Step 4: feed the Lagrange-MZT synth. The synth derives ω₀ internally
    // from `(freq_hz, sample_rate)` as `2π·fc/sr`, so pre-multiply `freq_hz`
    // by `omega_scale` to inject the per-band-type factor.
    let coeffs = proq4_s2_from_prototype_with_subfreq_pub(
        freq_hz * omega_scale,
        sample_rate,
        analog.b2z,
        analog.b1z,
        analog.b0z,
        analog.b2p,
        analog.b1p,
        analog.b0p,
        proto.wp,
        proto.wz,
        proto.wt,
        proto.w_eval,
    );

    (coeffs, fallback)
}

/// Inline fallback for section types without a dedicated helper.
///
/// Pro-Q's `prepare_band_display_info` else-branch sets
/// `proto[2] = wp·0.05`, `proto[3] = wp·0.5`. `wp` and `w_eval` are left
/// untouched.
pub fn apply_inline_section_defaults(proto: &mut Prototype) {
    proto.wz = proto.wp * 0.05;
    proto.wt = proto.wp * 0.5;
}

const _CONST_PI: f64 = PI;
