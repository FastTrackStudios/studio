//! Pro-Q 4's Magnitude-Matched Z-Transform (MZT) biquad design.
//!
//! Closed-form derivation matches the analog prototype exactly at DC, Nyquist,
//! and the corner frequency (3 points). This is the technique identified in
//! `compute_biquad_coefficients_from_poles` (0x1800fd7c0) from Pro-Q 4 binary RE.
//!
//! Formula for Peak/Bell biquad (derived in `docs/reports/proq4/re/biquad_design_algorithm.md`):
//!
//! ```text
//! x_0 = tan²(w_0_d / 2)           // pre-warped corner frequency
//! A = √(linear_gain)
//! D = (x_0 + 1) + √x_0 / (A·Q)
//! a_0 = 1
//! a_1 = 2(x_0 − 1) / D
//! a_2 = [(x_0 + 1) − √x_0 / (A·Q)] / D
//! b_0 = [(x_0 + 1) + √x_0 · A/Q] / D
//! b_1 = a_1                        (same as denominator middle)
//! b_2 = [(x_0 + 1) − √x_0 · A/Q] / D
//! ```
//!
//! This formulation matches Pro-Q 4's `compute_biquad_coefficients_from_poles` Mode 0
//! formula exactly with substitutions:
//!   `p_2` = 1, `p_3` = 1, `p_4` = `x_0`, `sp_5` = √`x_0` / (A·Q), `sp_6` = √`x_0` · A / Q.

mod bandpass;
mod cut;
mod notch;
mod peak;
mod shelf;

pub use bandpass::*;
pub use cut::*;
pub use notch::*;
pub use peak::*;
pub use shelf::*;

use crate::design::biquad::Coeffs;
use dsp_core::num;
use std::f64::consts::PI;

/// Generic MZT biquad design given analog prototype poles specified by
/// numerator and denominator "alpha" parameters.
///
/// Returns `[a0=1, a1, a2, b0, b1, b2]`.
///
/// Parameters:
/// - `w0_d`: digital corner angular frequency (radians/sample)
/// - `num_alpha`: analog numerator `α_n` such that (s² + `2·α_n·s` + `c_n`)/... at normalized w=1
/// - `den_alpha`: analog denominator `α_d`
/// - `num_const`: analog numerator constant term (e.g. A² for peak gain A)
/// - `den_const`: analog denominator constant term (1 for peak)
///
/// For Pro-Q 4 peak: `num_alpha = A/(2Q)`, `den_alpha = 1/(2·A·Q)`, both constants = 1.
#[must_use]
pub fn mzt_biquad(
    w0_d: f64,
    num_alpha: f64,
    den_alpha: f64,
    num_const: f64,
    den_const: f64,
) -> Coeffs {
    let x0 = (w0_d * 0.5).tan().powi(2); // tan²(w0/2)
    let x0_root = x0.sqrt(); // √x_0
    let xp1 = x0 + 1.0;

    // Denominator uses den_alpha, denominator constant den_const
    // Coefficients come from: a_k ∝ [xp1 ± √x0·(den_alpha·2·√den_const)]
    // For standard peak: den_alpha · 2·√den_const = 2·(1/(2·A·Q))·1 = 1/(A·Q)
    let d_mid = 2.0 * den_alpha * den_const.sqrt();
    let n_mid = 2.0 * num_alpha * num_const.sqrt();

    let denom = xp1 + x0_root * d_mid;
    let inv_d = 1.0 / denom;

    let a0 = 1.0;
    let a1 = 2.0 * (x0 - 1.0) * inv_d;
    let a2 = (xp1 - x0_root * d_mid) * inv_d;

    // Numerator: b-coefficients get the numerator's α (possibly different DC scale)
    // For DC gain = √(num_const/den_const), we need the numerator's DC mag factor.
    let dc_scale = (num_const / den_const).sqrt();
    let b0 = (xp1 * dc_scale + x0_root * n_mid) * inv_d;
    let b1 = 2.0 * (x0 - 1.0) * dc_scale * inv_d;
    let b2 = (xp1 * dc_scale - x0_root * n_mid) * inv_d;

    [a0, a1, a2, b0, b1, b2]
}

// ═══════════════════════════════════════════════════════════════════
// Filter-specific design functions
// ═══════════════════════════════════════════════════════════════════

/// Map analog quadratic (s² + α·w0·s + w0²) to digital via matched Z-transform.
/// Returns (c1, c2) where digital polynomial is `1 + c1·z⁻¹ + c2·z⁻²`.
pub(crate) fn mzt_quadratic(w0: f64, alpha: f64) -> (f64, f64) {
    let sigma = -w0 * alpha * 0.5;
    let disc = (alpha * alpha).mul_add(-0.25, 1.0);
    if disc >= 0.0 {
        // Underdamped: complex pair
        let omega = w0 * disc.sqrt();
        let r = sigma.exp();
        (-2.0 * r * omega.cos(), r * r)
    } else {
        // Overdamped: two real roots
        let rt = w0 * (-disc).sqrt();
        let z1 = (sigma + rt).exp();
        let z2 = (sigma - rt).exp();
        (-(z1 + z2), z1 * z2)
    }
}

/// HP slope=8 per-section biquad — Pro-Q 4 closed-form decoded from RE.
///
/// References:
/// - `docs/reports/proq4/re/hp_s8_w_eval_decoded.md` (sec 0)
/// - `docs/reports/proq4/re/hp_s8_w_eval_sec1_2_decoded.md` (sec 1, 2 + Nyquist guard)
///
/// LP slope=8 per-section closed-form biquad.
///
/// **Decoded 2026-05-01** (corrected) from `compute_audio_biquad_lagrange_mzt`
/// captures via `PROBE_HOOK_AUDIO_BIQUAD=1` at `filter_type=4` (= Pro-Q UI
/// "High Cut" = LP). Earlier traces at `filter_type=2` were misread — that ID
/// is "Low Cut" = HP, so the prior `LAG_OUT` data was actually HP s=8 (already
/// solved). With the correct `filter_type`, `AUDIO_BIQUAD` == `LAG_OUT` bit-exactly,
/// confirming both paths share `compute_audio_biquad_lagrange_mzt`.
///
/// Decoded prototype struct (`LAG_PROTO_DETAIL`) — **updated 2026-05-01**
/// from FT=4 captures in `docs/reports/proq4/re/lp_audio_path_captures/`
/// (48 cells: fc∈{10k,14k,16k,18k,21k,22k} × Q∈{0.5,1,4,10} × slope∈{2,8}):
/// - z-domain numerator: `b2z = 0, b1z = 0, b0z = 1` (LP form, constant)
/// - analog denominator: `b2p = 1, b1p = 1, b0p = 1` (LITERAL — generic
///   `s²+s+1`); the per-section Butterworth damping is carried in a
///   separate `w_section_field` slot.
/// - `w_section_field = 2·cos(θ_k)/Q_eff` where `θ_k` = (2k+1)π/24,
///
/// Closed-form decode of `compute_zpk_transfer_function_coefficients @ 0x1800fd420`
/// (281 bytes, Pro-Q 4 binary).
///
/// Decoded 2026-05-01 — see
/// `docs/reports/proq4/re/compute_zpk_transfer_function_coefficients_decoded.md`
/// for the bit-exact derivation. The function expands the analog prototype
/// `H(s) = (b2z·s² + b1z·s + b0z) / (b2p·s² + b1p·s + b0p)` evaluated at
/// `s = jω·g` into the squared-magnitude polynomial `(A·ω⁴+B·ω²+C) /
/// (D·ω⁴+E·ω²+F)`.
///
/// Two branches:
/// - `iVar4 == 2` (general second-order, fires when `|b2z| > 1.19e-7` OR
///   `|b2p| > 1.19e-7` at f32 precision): textbook expansion.
/// - `iVar4 == 1` (degree-1 numerator AND denominator simultaneously):
///   `A = D = 0`, `B = b1z²`, `C = b0z²·g²`, `E = b1p²`, `F = b0p²·g²`.
///
/// Verified bit-exact (≤1e-12 at f64 precision; 1.22e-6 against captured
/// values due to f32 rounding on `g` inside the binary) across 42 HP rows
/// and 78 LP rows in
/// `docs/reports/proq4/re/{hp_s8_proto_AF_capture, lp_AF_capture_failing_cells}.csv`.
///
/// Inputs: analog prototype `(b2z, b1z, b0z, b2p, b1p, b0p)` and frequency scale
/// `g` (= post-clamp ω₀). The b1p input MUST be the per-section
/// `w_section_field` value (= `2·cos(θ_k)/Q_eff` for cascaded LP/HP sections,
/// = √2/Q for slope=2 single-section) — NOT the literal `b1p=1` from the
/// `LAG_PROTO` template, which the binary overwrites at slot `+0x80` before
/// calling this function.
#[inline]
#[must_use]
#[expect(
    non_snake_case,
    reason = "parameter names match Pro-Q 4 analog prototype"
)]
pub fn zpk_section_to_AF(
    b2z: f64,
    b1z: f64,
    b0z: f64,
    b2p: f64,
    b1p: f64,
    b0p: f64,
    omega_scale: f64,
) -> (f64, f64, f64, f64, f64, f64) {
    const EPS_F32: f32 = 1.192_092_9e-7; // captured at 0x18023161c
    let abs_b2_zero_f32 = num::narrow(b2z).abs();
    let abs_b2_pole_f32 = num::narrow(b2p).abs();
    let second_order = abs_b2_zero_f32 > EPS_F32 || abs_b2_pole_f32 > EPS_F32;
    let g2 = omega_scale * omega_scale;
    let g4 = g2 * g2;
    if second_order {
        let num_b2sq = b2z * b2z;
        let num_cross = b1z.mul_add(b1z, -(2.0 * b2z * b0z)) * g2;
        let num_b0sq = b0z * b0z * g4;
        let den_b2sq = b2p * b2p;
        let den_cross = b1p.mul_add(b1p, -(2.0 * b2p * b0p)) * g2;
        let den_b0sq = b0p * b0p * g4;
        (num_b2sq, num_cross, num_b0sq, den_b2sq, den_cross, den_b0sq)
    } else {
        // First-order section: B has no g², C/F have a single g².
        let num_b2sq = 0.0;
        let num_cross = b1z * b1z;
        let num_b0sq = b0z * b0z * g2;
        let den_b2sq = 0.0;
        let den_cross = b1p * b1p;
        let den_b0sq = b0p * b0p * g2;
        (num_b2sq, num_cross, num_b0sq, den_b2sq, den_cross, den_b0sq)
    }
}

/// Sibling kernel that takes pre-computed `(A..F)` (e.g. from
/// [`zpk_section_to_AF`]) and feeds them through the
/// `compute_audio_biquad_lagrange_mzt` synth.
///
/// Branches in AFTER the `compute_zpk_transfer_function_coefficients` step.
///
/// The existing [`crate::design::cascade::proq4_s2_from_prototype_with_subfreq_pub`]
/// kernel takes `(b2z..b0p)` and computes `(A..F)` internally. This sibling
/// avoids that double-computation by accepting `(A..F)` directly. Useful for
/// validating the decoded `(A..F)` formula against captured values bit-exactly
/// without re-deriving them from the analog prototype.
///
/// **Note (2026-05-01):** The decoded formula matches what the existing kernel
/// already computes, so this sibling is currently scaffolding for future RE
/// iterations targeting the synthesis math (the actual LP s=2/s=8 blocker
/// is in the post-(A..F) Lagrange synthesis, not in (A..F) itself). See
/// `compute_zpk_transfer_function_coefficients_decoded.md` for the full
/// analysis.
#[expect(
    non_snake_case,
    reason = "parameter names (A, B, C, D, E, F) match the zpk transfer-function coefficients"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "a decoded routine's parameter list: each argument is one coefficient or pole term the reference implementation passes separately. Bundling them into a struct would rename the maths for no gain and break the correspondence with the decode notes"
)]
#[must_use]
pub fn proq4_s2_from_AF_with_subfreq(
    freq_hz: f64,
    sample_rate: f64,
    A: f64,
    B: f64,
    C: f64,
    D: f64,
    E: f64,
    F: f64,
    w_pole: f64,
    w_zero: f64,
    w_third: f64,
    w_eval: f64,
) -> Coeffs {
    const W_POLE_MAX: f64 = 3.078_760_800_517_997;
    const W_ZERO_MAX: f64 = 2.827_433_388_230_814;
    const W_THIRD_MAX: f64 = 3.063_366_996_515_407_3;

    let omega0_raw = 2.0 * PI * freq_hz / sample_rate;
    let omega0 = omega0_raw.min(PI - 0.01);

    let g_ref = if F.abs() > 1e-300 { C / F } else { 0.0 };

    let w_pole = w_pole.min(W_POLE_MAX);
    let w_zero = w_zero.min(W_ZERO_MAX);
    let w_third = w_third.min(W_THIRD_MAX);
    let w_eval = w_eval.clamp(0.0, PI);

    let h_sq = |w: f64| -> f64 {
        let w2 = w * w;
        let w4 = w2 * w2;
        let num = A.mul_add(w4, B.mul_add(w2, C));
        let den = D.mul_add(w4, E.mul_add(w2, F));
        if den.abs() > 1e-300 { num / den } else { 0.0 }
    };
    let u_pole = h_sq(w_pole);
    let u_zero = h_sq(w_zero);
    let u_third = h_sq(w_third);
    let u_eval = h_sq(w_eval);

    let p3 = u_eval.max(0.0).sqrt();
    let p2 = g_ref.max(0.0).sqrt();

    let t1 = (w_pole * 0.5).tan();
    let t2 = (w_zero * 0.5).tan();
    let t3 = (w_third * 0.5).tan();
    let t1s = t1 * t1;
    let t2s = t2 * t2;
    let t3s = t3 * t3;

    let den = t3s
        * ((u_zero - u_third) * (g_ref - u_pole))
            .mul_add(t2s, -((u_pole - u_third) * (g_ref - u_zero) * t1s))
        + (g_ref - u_third) * (u_pole - u_zero) * t1s * t2s;
    let num = ((t2s - t3s) * u_third).mul_add(
        u_zero,
        u_pole * (t3s - t1s).mul_add(u_third, (t2s - t3s).mul_add(u_eval, (t1s - t2s) * u_zero))
            + u_eval * (t3s - t1s).mul_add(u_zero, (t1s - t2s) * u_third),
    );

    let s2 = if den.abs() > 1e-30 {
        (num / den).max(0.0)
    } else {
        0.0
    };
    let s_val = s2.sqrt();
    let p4 = s_val * t1 * t2 * t3;

    let a1_term = t1s * p3 - p4 * p2;
    let a2_term = t2s * p3 - p4 * p2;

    let sp6_den = (u_pole - u_zero) * t1s * t2s;
    let sp6 = if sp6_den.abs() > 1e-30 {
        let sp6_num = (a1_term * a1_term * t2s).mul_add(
            u_zero,
            -((t1s * t2s * (1.0 - s2 * t3s) * (t1s - t2s))
                .mul_add(u_zero, a2_term * a2_term * t1s)
                * u_pole),
        );
        (sp6_num / sp6_den).max(0.0)
    } else {
        0.0
    };
    let sp5 = if (t1s * u_pole).abs() > 1e-30 {
        ((sp6 * t1s - (t1s - p4).powi(2) * u_pole + a1_term * a1_term) / (t1s * u_pole)).max(0.0)
    } else {
        0.0
    };

    let sq5 = sp5.sqrt();
    let sq6 = sp6.sqrt();
    let big_d = (1.0 + p4) + sq5;
    if !big_d.is_finite() || big_d.abs() < 1e-30 {
        return crate::design::biquad::PASSTHROUGH;
    }
    let inv_d = 1.0 / big_d;
    let b0 = (p2 * p4 + p3 + sq6) * inv_d;
    let b1 = -2.0 * (p3 - p2 * p4) * inv_d;
    let b2 = (p3 - sq6 + p2 * p4) * inv_d;
    let a1 = -2.0 * (1.0 - p4) * inv_d;
    let a2 = (1.0 + p4 - sq5) * inv_d;
    let _ = omega0; // currently unused — reserved for future post-AF correction

    [1.0, a1, a2, b0, b1, b2]
}

/// Pro-Q 4's `compute_biquad_coefficients_from_poles` Mode 0 formula —
/// thin wrapper over [`crate::design::biquad::Mode0Params::to_biquad`] for legacy
/// positional call sites in this module.
#[inline]
pub(crate) fn biquad_from_mode0_params(p2: f64, p3: f64, p4: f64, sp5: f64, sp6: f64) -> Coeffs {
    crate::design::biquad::Mode0Params {
        p2,
        p3,
        p4,
        sp5,
        sp6,
    }
    .to_biquad()
}
