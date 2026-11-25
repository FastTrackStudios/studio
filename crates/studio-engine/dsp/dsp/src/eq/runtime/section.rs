//! TDF2 biquad processing section with Pro-Q 4's double-precision history.

use crate::eq::design::biquad::Coeffs;

const MAX_CH: usize = 2;

use dsp_core::{Channel, PerChannel};

/// One channel's transposed-direct-form-II state.
#[derive(Debug, Clone, Copy)]
struct Tdf2State {
    s1: f64,
    s2: f64,
}

/// One channel's direct-form-I history.
#[derive(Debug, Clone, Copy)]
struct Df1State {
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Df1State {
    const ZERO: Self = Self {
        x1: 0.0,
        x2: 0.0,
        y1: 0.0,
        y2: 0.0,
    };
}

/// Transposed Direct Form II biquad section.
///
/// Double-precision state matching Pro-Q 4's internal processing path.
#[derive(Clone)]
pub struct Tdf2Section {
    c0: f64, // b0/a0
    c1: f64, // b1/a0
    c2: f64, // b2/a0
    c3: f64, // a1/a0
    c4: f64, // a2/a0
    state: PerChannel<Tdf2State>,
}

impl Tdf2Section {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            c0: 1.0,
            c1: 0.0,
            c2: 0.0,
            c3: 0.0,
            c4: 0.0,
            state: PerChannel::filled(Tdf2State { s1: 0.0, s2: 0.0 }),
        }
    }

    /// Load biquad coefficients from [a0, a1, a2, b0, b1, b2] format.
    pub fn set_coeffs(&mut self, coeffs: Coeffs) {
        let a0_inv = 1.0 / coeffs[0];
        self.c0 = coeffs[3] * a0_inv; // b0/a0
        self.c1 = coeffs[4] * a0_inv; // b1/a0
        self.c2 = coeffs[5] * a0_inv; // b2/a0
        self.c3 = coeffs[1] * a0_inv; // a1/a0
        self.c4 = coeffs[2] * a0_inv; // a2/a0
    }

    /// The normalised coefficients, `[b0, b1, b2, a1, a2]` with `a0 = 1`.
    ///
    /// Wanted for evaluating the cascade's response without redesigning it —
    /// the auto-gain compensation has to know what curve the chain is
    /// currently applying.
    #[must_use]
    pub const fn coeffs(&self) -> [f64; 5] {
        [self.c0, self.c1, self.c2, self.c3, self.c4]
    }

    /// Process one sample through the biquad (TDF2).
    #[inline]
    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let state = &mut self.state[Channel::new(ch.min(MAX_CH - 1))];
        let output = input.mul_add(self.c0, state.s1);
        state.s1 = input.mul_add(self.c1, -(output * self.c3)) + state.s2;
        state.s2 = input.mul_add(self.c2, -(output * self.c4));
        output
    }

    /// Reset all state to zero.
    pub const fn reset(&mut self) {
        self.state.fill(Tdf2State { s1: 0.0, s2: 0.0 });
    }
}

impl Default for Tdf2Section {
    fn default() -> Self {
        Self::new()
    }
}

/// Direct Form I biquad — numerically stable alternative to TDF2.
///
/// From Pro-Q 4 binary (`process_biquad_cascade_single_channel` @ 0x18011ffc0).
/// Processes standard biquad H(z) = (b0+b1z^-1+b2z^-2)/(1+a1z^-1+a2z^-2)
/// using Direct Form I which is more stable than TDF2 for high-Q near Nyquist.
///
/// Direct Form I difference equation:
///   y[n] = b0*x[n] + b1*x[n-1] + b2*x[n-2] - a1*y[n-1] - a2*y[n-2]
#[derive(Clone)]
pub struct Df1Section {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    state: PerChannel<Df1State>,
}

impl Df1Section {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            state: PerChannel::filled(Df1State::ZERO),
        }
    }

    /// Load biquad coefficients from [a0, a1, a2, b0, b1, b2] format.
    pub fn set_coeffs(&mut self, coeffs: Coeffs) {
        let a0_inv = 1.0 / coeffs[0];
        self.b0 = coeffs[3] * a0_inv;
        self.b1 = coeffs[4] * a0_inv;
        self.b2 = coeffs[5] * a0_inv;
        self.a1 = coeffs[1] * a0_inv;
        self.a2 = coeffs[2] * a0_inv;
    }

    /// The normalised coefficients, `[b0, b1, b2, a1, a2]` with `a0 = 1`.
    #[must_use]
    pub const fn coeffs(&self) -> [f64; 5] {
        [self.b0, self.b1, self.b2, self.a1, self.a2]
    }

    /// Process one sample through the biquad (Direct Form I).
    #[inline]
    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let state = &mut self.state[Channel::new(ch.min(MAX_CH - 1))];
        let output = self.a2.mul_add(
            -state.y2,
            self.a1.mul_add(
                -state.y1,
                self.b2
                    .mul_add(state.x2, self.b0.mul_add(input, self.b1 * state.x1)),
            ),
        );
        state.x2 = state.x1;
        state.x1 = input;
        state.y2 = state.y1;
        state.y1 = output;
        output
    }

    /// Reset all state to zero.
    pub const fn reset(&mut self) {
        self.state.fill(Df1State::ZERO);
    }
}

impl Default for Df1Section {
    fn default() -> Self {
        Self::new()
    }
}
