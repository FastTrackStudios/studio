//! Pro-Q 4's cascade coefficient computation for peak/bell and shelf (type 12) filters.
//!
//! `compute_cascade_coefficients` (0x1800fec20) computes ZPK directly for peak/bell
//! filters without going through Butterworth prototypes. It uses a specialized approach:
//!
//! - For type 0 (peak/bell): RBJ cookbook with per-section gain distribution.
//!   Higher orders distribute gain across sections with exponential spacing.
//!
//! - For type 0xc (shelf alt / type 12): gain = sqrt(gain), with geometric gain
//!   spacing across sections for smooth shelf transitions.
//!
//! Key insight: Pro-Q 4 does NOT simply stack identical biquads. Each section gets
//! a different `gain_db/section` to create the proper cascade response.

use crate::eq::inline::{InlineVec, inline_vec};

use std::f64::consts::PI;

use crate::eq::design::biquad::{Coeffs, PASSTHROUGH};

mod bandpass;
mod brickwall;
mod lagrange;
mod notch;
mod s2;
mod shelf_alt;
mod tables;

pub use bandpass::*;
pub use notch::*;
pub use shelf_alt::*;

// Implementation split across the files above. The module's surface is
// deliberately unchanged, so every existing `cascade::foo` call site still
// resolves and this split stays a pure move.
pub(crate) use brickwall::{
    bell_brickwall_cascade, bell_brickwall_proq4, bell_brickwall_proq4_n, bell_three_point_synth,
};
pub(crate) use lagrange::lagrange_synth_alt_path;
pub(crate) use s2::{
    bell_s2_proq4, highpass_s2_proq4, lowpass_s2_proq4, mode0_forward,
    proq4_s2_from_prototype_with_subfreq, proq4_s2_from_prototype_with_subfreq_pub,
};
pub(crate) use tables::bp_cascade_for_q;

/// Compute cascade biquads for a peak/bell filter.
///
/// Uses Vicanek matched peak EQ with per-section gain distribution.
/// Each section gets `gain_db/N` dB with the same user Q.
///
/// Pro-Q 4 binary (`compute_cascade_coefficients` @ 0x1800fec20) uses a
/// Butterworth zero cascade at angles `θ_k` = π(2k+1)/(2·order) with gain
/// accumulation ∏ `0.25/cos²(θ_k)`. The exact multi-section Q mapping is
/// complex and not yet fully extracted. The Vicanek approach gives 99.3%
/// parity for single/dual sections and ~65% for higher orders.
#[must_use]
pub fn compute_cascade_peak(
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    order: usize,
) -> InlineVec<Coeffs> {
    compute_cascade_peak_with_slope(freq_hz, q, gain_db, sample_rate, order, None)
}

#[must_use]
pub fn compute_cascade_peak_with_slope(
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    order: usize,
    slope_idx: Option<usize>,
) -> InlineVec<Coeffs> {
    let n = (order / 2).max(1);

    if gain_db.abs() < 0.001 {
        return inline_vec![PASSTHROUGH; n];
    }

    let _ = slope_idx;
    if n == 1 {
        return inline_vec![bell_s2_proq4(freq_hz, q, gain_db, sample_rate)];
    }

    // n ≥ 2: legacy LP→BP→BLT cascade — 68/416 baseline on s=5 / s=8.
    //
    // Closed-form `bell_brickwall_proq4` (below) is 36/56 sections bit-exact
    // (≤ 1e-12) at the per-section synthesis stage against
    // `docs/reports/proq4/re/lagrange_per_section_sweep.csv`, but routing it
    // here regressed cascade-product conformance to 0/416 on s=5 / s=8.
    // Pro-Q 4 dispatches each section into either the Bell 3-point Lagrange
    // branch (`compute_audio_biquad_lagrange_mzt @ 0x180110855`) or the
    // brickwall 2-point alt-path (`@ 0x180110728`) based on
    // `byte[proto+0x48]`, which empirically tracks `w_third == 0` (alt-path)
    // vs `w_third != 0` (Bell).  The current closed form forces alt-path for
    // every section, producing wrong peak placement at user fc — see
    // `tools/proq4_probe/verify_brickwall_per_section.py`.  Hold on legacy
    // until the upstream selector + per-section `w_third` is decoded.
    let _ = bell_brickwall_proq4_n;
    let _ = bell_brickwall_cascade;
    // Try the closed-form bucket-B path. n_sections matches the section count
    // we have captured per slope (s=3/4 → 2, s=5/6 → 3, s=7 → 4, s=8 → 6,
    // s=9 → 8). When pole_count comes from slope_from_order this is correct.
    bell_brickwall_proq4(freq_hz, q, gain_db, sample_rate, n, slope_idx)
}

/// Bell brickwall closed form for slope ≥ 4.
///
/// Pipeline (per Ghidra RE + capture analysis 2026-05-01):
///
/// **Analog ZPK structure (verified slope=4, all captured (fc, Q, g)):**
/// - The analog prototype is a Butterworth bandpass obtained via the
///   classical LP→BP transform `s → Q·(s/ω₀ + ω₀/s)` applied to a
///   Butterworth LP of order `N_LP = slope/2`.
/// - For each LP-pole `p_LP_k = e^(j(π − θ_k))` (`θ_k` = `π(2k+1)/(2·N_LP)`),
///   the LP→BP transform yields *two* BP poles solving
///   `s² − (p_LP/Q')·s + 1 = 0`, with reciprocal magnitudes (one inside
///   the unit circle, one outside).  The two sections per LP-pole
///   correspond to the two reciprocal roots.
/// - **Pole/zero gain split** (decoded from C/F = `g_ref` symmetry):
///     - boost (`g_dB` > 0): `Q'_pole = Q`,  `Q'_zero = Q / √g_lin`
///     - cut   (`g_dB` < 0): `Q'_pole = Q · √g_lin`, `Q'_zero = Q`
/// - The `(A, B, C, D, E, F)` polynomial in `ω` (digital rad/sample)
///   is built from these analog s-plane (b2=1) quadratics with
///   `(A,D)=1`, `(B,E)=(b1²−2b0)·ω₀²`, `(C,F)=b0²·ω₀⁴`.
/// - Match against `solve_bq_sweep.csv` (slope=4, 256 rows): structurally
///   exact, but residual ≤ 0.5% per coefficient — Pro-Q applies an
///   additional Q-correction on the LP→BP `Q'` that has not yet been
///   bit-exactly decoded (likely involves the elliptic LP→BP variant
///   used elsewhere in the binary, see `prototype.rs::butterworth_bp_elliptic`).
///
/// **Sub-frequency derivation:**
/// - `w_pole` and `w_third` are the two positive roots of the |H(jω)|²
///   peak-finder quadratic in `u = ω²`:
///   `(A·E − B·D)·u² + 2(A·F − C·D)·u + (B·F − C·E) = 0`.
/// - Verified bit-exact (≤ 1e-15) against `solve_bq_sweep.csv` for all
///   captured slope=4 rows (`root_count=2` column).  The smaller root is
///   `w_pole_solve`, the larger is `w_third_solve`.
/// - **Caveat**: for some sections (e.g. sec=0 in fc=500/Q=1) the
///   `persec.w_pole` used by the audio synthesis is *not* the smaller
///   solve root.  An additional per-section selection layer maps the
///   peak-finder roots into the (`w_pole`, `w_zero`, `w_third`) triple fed to
///   the Lagrange synth.  This selection is undecoded; the captures
///   suggest a recipe involving `(w_pole, w_zero, w_third) ≈ ω₀·(α, α/100, α/10)`
///   for one branch and `(big_root, mid, big_root/10)` for the other.
///
/// **Per-section dispatch (verified):**
/// - `w_third == 0` → call `lagrange_synth_alt_path` (2-point alt-path).
/// - `w_third != 0` → call the Bell 3-point Lagrange (same body as
///   `bell_s2_proq4` post sub-frequency selection).
/// - `tools/proq4_probe/verify_brickwall_per_section.py` confirms 60/64
///   (≤ 1e-15) and 4/64 (≤ 1.9e-13) when *correct* sub-frequencies are
///   supplied; the failure mode of routing this to `compute_cascade_peak`
///   is upstream sub-frequency derivation, not the synth itself.
///
/// **Status:** held off the audio path — `compute_cascade_peak` still
/// dispatches into the legacy `bell_brickwall_cascade` table-based
/// path (68/416 baseline on s=5/s=8) until the (a) elliptic-corrected
/// `Q'`, and (b) per-section sub-frequency selection are decoded.
/// Whether the `FTSEQ_TRACE_BELL_INPUTS` debug trace is on.
///
/// Read once. The raw `env::var` was on the per-call bell path, where it takes
/// a process-wide lock and allocates every time a coefficient is designed.
fn trace_bell_inputs() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    #[expect(
        clippy::disallowed_methods,
        reason = "read exactly once through OnceLock, not per call"
    )]
    *ON.get_or_init(|| std::env::var("FTSEQ_TRACE_BELL_INPUTS").is_ok())
}
