//! Frequency domain transforms matching Pro-Q 4's transform type table at 0x18020eff0.
//!
//! Transform types:
//!   0 = Direct (no transform) — LP/HP
//!   1 = LP→BP frequency warp — BP/Notch
//!   2 = Bilinear transform — Shelves
//!   3 = LP→BP + bilinear — Band Shelf
//!   4 = Negate zeros — Allpass

use crate::eq::inline::InlineVec;

use crate::eq::math::zpk::{Complex, Zpk};

#[expect(
    clippy::arithmetic_side_effects,
    reason = "complex/float arithmetic — `Complex` is two `f64`s, so its operators cannot panic or overflow; the lint cannot see through an operator overload"
)]
/// Bilinear s→z transform (transform type 2).
///
/// Maps analog prototype poles/zeros from the s-plane to the z-plane using:
///   z = (1 + s/2fs) / (1 - s/2fs)
///
/// Unmapped zeros are placed at z = -1 (Nyquist) to preserve filter order.
#[must_use]
pub fn bilinear(zpk: &Zpk, sample_rate: f64) -> Zpk {
    let fs2 = 2.0 * sample_rate;
    let _fs2_complex = Complex::new(fs2, 0.0);

    let mut z_zeros: InlineVec<Complex> = zpk
        .zeros
        .iter()
        .map(|&z| {
            let num = Complex::new(1.0 + z.re / fs2, z.im / fs2);
            let den = Complex::new(1.0 - z.re / fs2, -z.im / fs2);
            let inv_den = den.inv();
            num * inv_den
        })
        .collect();

    let z_poles: InlineVec<Complex> = zpk
        .poles
        .iter()
        .map(|&p| {
            let num = Complex::new(1.0 + p.re / fs2, p.im / fs2);
            let den = Complex::new(1.0 - p.re / fs2, -p.im / fs2);
            let inv_den = den.inv();
            num * inv_den
        })
        .collect();

    // Pad zeros to match pole count — extra zeros go to Nyquist (z = -1).
    while z_zeros.len() < z_poles.len() {
        z_zeros.push(Complex::new(-1.0, 0.0));
    }

    // Adjust gain: product of (fs2 - z) / product of (fs2 - p).
    let mut gain = Complex::new(zpk.gain, 0.0);
    for &z in &zpk.zeros {
        let diff = Complex::new(fs2, 0.0) - z;
        gain = gain * diff;
    }
    for &p in &zpk.poles {
        gain = gain / (Complex::new(fs2, 0.0) - p);
    }
    // Account for added Nyquist zeros.
    let extra = z_poles.len().saturating_sub(zpk.zeros.len());
    for _ in 0..extra {
        gain = gain * Complex::new(fs2, 0.0);
    }

    Zpk::new(z_zeros, z_poles, gain.re)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "complex/float arithmetic — `Complex` is two `f64`s, so its operators cannot panic or overflow; the lint cannot see through an operator overload"
)]
/// Allpass transform (transform type 4): negate real part of poles to get zeros.
///
/// From Pro-Q 4 binary (`design_filter_zpk_and_transform`, transform type 4):
/// Each pole's REAL part is negated to create the corresponding zero:
///   zero = Complex(-pole.re, pole.im)
///
/// This is equivalent to reflecting across the imaginary axis, which for
/// z-plane poles inside the unit circle creates zeros outside it, giving
/// |H(e^jw)| = 1 for all frequencies.
#[must_use]
pub fn make_allpass(zpk: &Zpk) -> Zpk {
    let zeros: InlineVec<Complex> = zpk
        .poles
        .iter()
        .map(|&p| {
            // Negate real part: zero = -conj(pole) reflected across imaginary axis
            Complex::new(-p.re, p.im)
        })
        .collect();

    // Normalize gain so |H(e^jw)| = 1: evaluate at DC (z=1) and scale
    // For allpass with negated-real zeros: gain must compensate for the
    // pole/zero magnitude imbalance
    let mut gain = zpk.gain;
    for (&p, &z) in zpk.poles.iter().zip(zeros.iter()) {
        // At z=1 (DC): H = gain * prod(1-z_i) / prod(1-p_i)
        // We want |H(1)| = 1
        let num_dc = (Complex::ONE - z).mag();
        let den_dc = (Complex::ONE - p).mag();
        if den_dc > 1e-15 && num_dc > 1e-15 {
            gain *= den_dc / num_dc;
        }
    }

    Zpk::new(zeros, zpk.poles.clone(), gain)
}
