//! Hadamard mixing matrix for FDN.
//!
//! Recursive in-place Hadamard transform using only additions
//! and subtractions (no multiplications). Energy-preserving when
//! normalized by `1/sqrt(N)`.
//!
//! Supports N = power of 2 (2, 4, 8, 16).

use dsp_core::num;

/// In-place Hadamard transform with normalization.
///
/// `channels.len()` must be a power of 2.
#[inline]
pub fn mix(channels: &mut [f64]) {
    let n = channels.len();
    debug_assert!(n.is_power_of_two(), "Hadamard requires power-of-2 size");

    // Recursive butterfly
    let mut half = n;
    while half > 1 {
        half >>= 1;
        // Each butterfly block is `2 * half` wide; splitting it in two
        // pairs j with j + half without any index arithmetic. The `min`
        // only matters for a non-power-of-2 length, where the last chunk
        // is short and the extra entries stay untouched.
        for block in channels.chunks_mut(half.saturating_mul(2)) {
            let (top, bottom) = block.split_at_mut(half.min(block.len()));
            for (upper, lower) in top.iter_mut().zip(bottom.iter_mut()) {
                let (a, b) = (*upper, *lower);
                *upper = a + b;
                *lower = a - b;
            }
        }
    }

    // Normalize to preserve energy
    let scale = 1.0 / num::count_to_f64(n).sqrt();
    for ch in channels.iter_mut() {
        *ch *= scale;
    }
}
