//! Householder reflection matrix for FDN mixing.
//!
//! `A = I - (2/N) * u * u^T` where `u = [1, 1, ..., 1]^T`
//!
//! This is a unitary (energy-preserving) matrix that cross-couples
//! all delay lines with only 2N-1 additions — no multiplications
//! except the final scaling.

use dsp_core::num;

/// Apply Householder reflection in-place.
///
/// For N channels: compute mean, then `out[i] = 2*mean - in[i]`.
/// This is equivalent to `I - (2/N)*ones*ones^T`.
#[inline]
pub fn mix(channels: &mut [f64]) {
    let n = channels.len();
    if n == 0 {
        return;
    }
    let sum: f64 = channels.iter().sum();
    let scale = 2.0 / num::count_to_f64(n);
    for ch in channels.iter_mut() {
        *ch = sum.mul_add(scale, -*ch);
    }
}
