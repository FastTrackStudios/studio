//! Elliptic functions for LP->BP / LP->BS pole placement.
//!
//! Pro-Q 4 uses these for bandpass and notch filter design where each biquad
//! section receives UNIQUE pole/zero positions via elliptic function mapping.
//!
//! Functions extracted from:
//!   - `elliptic_k_complete` (0x18011eb50) -- complete elliptic integral K(m)
//!   - `elliptic_sn`         (0x18011e6f0) -- Jacobi elliptic sn(u,k)
//!   - `elliptic_asn`        (0x18011e900) -- inverse Jacobi sn

use dsp_core::num;
use std::f64::consts::PI;

/// Maximum iterations for iterative algorithms.
const MAX_ITER: usize = 64;

/// Convergence tolerance.
const TOL: f64 = 1e-15;

/// Complete elliptic integral of the first kind K(m), where m = k^2.
///
/// Uses the arithmetic-geometric mean (AGM) method:
///   `a_0` = 1,  `b_0` = `sqrt(1 - m)`
///   `a_n` = (`a_{n-1}` + `b_{n-1}`) / 2
///   `b_n` = `sqrt(a_{n-1} * b_{n-1})`
///   `K(m)` = pi / (2 * `a_final`)
///
/// Matches Pro-Q 4 function at 0x18011eb50.
#[must_use]
pub fn elliptic_k_complete(m: f64) -> f64 {
    if m >= 1.0 {
        return f64::INFINITY;
    }
    if m < 0.0 {
        return f64::NAN;
    }

    let mut a = 1.0;
    let mut b = (1.0 - m).sqrt();

    for _ in 0..MAX_ITER {
        let a_next = (a + b) * 0.5;
        let b_next = (a * b).sqrt();

        if (a_next - b_next).abs() < TOL * a_next {
            return PI / (2.0 * a_next);
        }

        a = a_next;
        b = b_next;
    }

    PI / (2.0 * a)
}

/// Jacobi elliptic function sn(u, k) via AGM descent (Abramowitz & Stegun 16.4).
///
/// 1. Compute AGM sequence: `a_0`=1, `b_0`=`sqrt(1-m)`, `c_0`=k
/// 2. Compute `phi_N` = 2^N * `a_N` * u, then descend:
///    `phi_{n-1}` = (`phi_n` + `arcsin(c_n / a_n * sin(phi_n))`) / 2
/// 3. `sn(u,k)` = `sin(phi_0)`
///
/// Matches Pro-Q 4 function at 0x18011e6f0.
#[must_use]
pub fn elliptic_sn(u_input: f64, modulus: f64) -> f64 {
    if modulus.abs() < TOL {
        return u_input.sin();
    }
    if (modulus.abs() - 1.0).abs() < TOL {
        return u_input.tanh();
    }

    let m_param = modulus * modulus;

    // Build AGM sequences.
    let mut a_seq = smallvec::SmallVec::<[f64; MAX_ITER]>::new();
    let mut c_seq = smallvec::SmallVec::<[f64; MAX_ITER]>::new();

    let mut agm_a = 1.0;
    let mut agm_b = (1.0 - m_param).sqrt();

    a_seq.push(agm_a);
    c_seq.push(modulus.abs());

    let mut iter_count: usize = 0;
    for _ in 0..MAX_ITER {
        let a_next = (agm_a + agm_b) * 0.5;
        let c_next = (agm_a - agm_b) * 0.5;
        let b_next = (agm_a * agm_b).sqrt();
        iter_count = iter_count.saturating_add(1);
        a_seq.push(a_next);
        c_seq.push(c_next);

        if c_next.abs() < TOL {
            agm_a = a_next;
            break;
        }

        agm_a = a_next;
        agm_b = b_next;
    }

    // phi_N = 2^N * a_N * u
    let two_pow_n = 2.0_f64.powi(i32::try_from(iter_count).unwrap_or(31));
    let mut phi = two_pow_n * agm_a * u_input;

    // Descend: phi_{n-1} = (phi_n + arcsin(c_n/a_n * sin(phi_n))) / 2
    for i in (1..=iter_count).rev() {
        let c_i = c_seq.get(i).copied().unwrap_or(0.0);
        let a_i = a_seq.get(i).copied().unwrap_or(1.0);
        phi = (phi + (c_i / a_i * phi.sin()).asin()) * 0.5;
    }

    phi.sin()
}

/// Inverse Jacobi elliptic function: returns u such that sn(u, k) = y.
///
/// Uses Newton's method on f(u) = sn(u, k) - y, with derivative
/// f'(u) = cn(u,k) * dn(u,k).
///
/// Matches Pro-Q 4 function at 0x18011e900.
#[must_use]
pub fn elliptic_asn(y: f64, k: f64) -> f64 {
    if k.abs() < TOL {
        return y.asin();
    }
    if (k.abs() - 1.0).abs() < TOL {
        return y.atanh();
    }

    // Initial guess: arcsin(y) is exact when k = 0.
    let mut u = y.clamp(-1.0, 1.0).asin();

    for _ in 0..MAX_ITER {
        let sn = elliptic_sn(u, k);
        let residual = sn - y;

        if residual.abs() < TOL {
            break;
        }

        // d(sn)/du = cn * dn, where cn = sqrt(1 - sn^2), dn = sqrt(1 - k^2 * sn^2)
        let cn = (1.0 - sn * sn).max(0.0).sqrt();
        let dn = (k * k * sn).mul_add(-sn, 1.0).max(0.0).sqrt();
        let deriv = cn * dn;

        if deriv.abs() < 1e-30 {
            break;
        }

        u -= residual / deriv;
    }

    u
}

/// Jacobi sn, cn and dn together, for real `u` and modulus `k`.
///
/// The AGM descent that [`elliptic_sn`] runs already produces the amplitude
/// `phi`; all three functions fall straight out of it, and taking them from
/// one descent keeps them consistent with each other. Deriving `cn` as
/// `sqrt(1 - sn^2)` instead loses its sign, which matters here — the elliptic
/// prototype evaluates `cd` at arguments past `K` where `cn` is negative.
#[must_use]
pub fn elliptic_sncndn(u_input: f64, modulus: f64) -> (f64, f64, f64) {
    if modulus.abs() < TOL {
        return (u_input.sin(), u_input.cos(), 1.0);
    }
    if (modulus.abs() - 1.0).abs() < TOL {
        let t = u_input.tanh();
        let sech = 1.0 / u_input.cosh();
        return (t, sech, sech);
    }

    let m_param = modulus * modulus;
    let mut a_seq = smallvec::SmallVec::<[f64; MAX_ITER]>::new();
    let mut c_seq = smallvec::SmallVec::<[f64; MAX_ITER]>::new();
    let mut agm_a = 1.0;
    let mut agm_b = (1.0 - m_param).sqrt();
    a_seq.push(agm_a);
    c_seq.push(modulus.abs());

    let mut iter_count: usize = 0;
    for _ in 0..MAX_ITER {
        let a_next = (agm_a + agm_b) * 0.5;
        let c_next = (agm_a - agm_b) * 0.5;
        let b_next = (agm_a * agm_b).sqrt();
        iter_count = iter_count.saturating_add(1);
        a_seq.push(a_next);
        c_seq.push(c_next);
        if c_next.abs() < TOL {
            agm_a = a_next;
            break;
        }
        agm_a = a_next;
        agm_b = b_next;
    }

    let mut phi = num::u64_to_f64(1u64 << iter_count) * agm_a * u_input;
    for i in (1..=iter_count).rev() {
        let c_i = c_seq.get(i).copied().unwrap_or(0.0);
        let a_i = a_seq.get(i).copied().unwrap_or(1.0);
        phi = (phi + (c_i / a_i * phi.sin()).asin()) * 0.5;
    }
    let sn = phi.sin();
    let cn = phi.cos();
    // dn is strictly positive for real u, so the square root is unambiguous.
    let dn = (m_param * sn).mul_add(-sn, 1.0).max(0.0).sqrt();
    (sn, cn, dn)
}

/// Solve the elliptic degree equation: the modulus `k` for which an order-`n`
/// elliptic filter with modulus ratio `k1` has its transition band.
///
/// `k = omega_p / omega_s`, so a smaller `k1` (deeper stopband, flatter
/// passband) or a lower order both push `k` down and the transition wider.
///
/// Solved through the nome rather than by iteration: `q1 = exp(-pi K'/K)` at
/// `k1`, then `q = q1^(1/n)`, then `k` back out of `q` by the theta-series
/// ratio. The series converge geometrically in `q`, which is tiny for any
/// stopband worth having, so four terms are already at machine precision.
#[must_use]
pub fn ellipdeg(n: usize, k1: f64) -> f64 {
    let k1 = k1.clamp(1e-300, 1.0 - 1e-15);
    let big_k = elliptic_k_complete(k1 * k1);
    let big_kp = elliptic_k_complete(1.0 - k1 * k1);
    let q1 = (-PI * big_kp / big_k).exp();
    let q = q1.powf(1.0 / num::count_to_f64(n));

    let (mut num, mut den) = (1.0f64, 1.0f64);
    for m in 1..=8u32 {
        let exp_m_next = m.saturating_mul(m.saturating_add(1));
        num += q.powi(i32::try_from(exp_m_next).unwrap_or(i32::MAX));
        let exp_m_sq = m.saturating_mul(m);
        den += 2.0 * q.powi(i32::try_from(exp_m_sq).unwrap_or(i32::MAX));
    }
    4.0 * q.sqrt() * (num / den).powi(2)
}
