//! Gain reduction smoothing with attack/release time constants.

use dsp_core::{Channel, PerChannel};

use dsp_core::audio::envelope::EnvelopeFollower;

/// Exponential smoother for gain reduction.
///
/// State lives in shared [`EnvelopeFollower`]s (one per channel). The
/// coefficient formula is comp-specific — `exp(-2 / (sr * t))` applied to
/// the *input* term rather than the state term — so coefficients are
/// mapped as `1 - coeff` when handed to the follower, which computes
/// `coeff * (state - input) + input`. Numerics are identical to the
/// original hand-rolled implementation.
pub struct GainReductionSmoother {
    sample_rate: f64,
    attack_s: f64,
    release_s: f64,
    /// Per-channel smoothing state.
    env: PerChannel<EnvelopeFollower>,
}

impl GainReductionSmoother {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let mut s = Self {
            sample_rate,
            attack_s: 0.01,
            release_s: 0.05,
            env: PerChannel::new(
                [(); dsp_core::channel::MAX_CHANNELS].map(|()| EnvelopeFollower::new(1.0)),
            ),
        };
        s.update_coeffs();
        s
    }

    pub fn set_attack(&mut self, attack_s: f64) {
        self.attack_s = attack_s.max(0.0001);
        self.update_coeffs();
    }

    pub fn set_release(&mut self, release_s: f64) {
        self.release_s = release_s.max(0.001);
        self.update_coeffs();
    }

    fn update_coeffs(&mut self) {
        // EnvelopeFollower weights its *state* by coeff; the original
        // formula weighted the *input* by exp(-2/(sr*t)) — hence 1 - c.
        let attack = 1.0 - self.compute_coeff(self.attack_s);
        let release = 1.0 - self.compute_coeff(self.release_s);
        for e in &mut self.env {
            e.set_coeffs(attack, release);
        }
    }

    /// Smooth GR using exponential smoothing with attack/release coefficients.
    ///
    /// Attack when GR rises (`gr_inst >= state`), release when it falls —
    /// the follower's `input > value` test matches on the rising side, and
    /// at exact equality both branches produce the same output.
    pub fn smooth_gr(&mut self, gr_inst: f64, ch: Channel) -> f64 {
        self.env[ch].tick(gr_inst)
    }

    #[inline]
    fn compute_coeff(&self, time_s: f64) -> f64 {
        const SCALE: f64 = 2.0;
        (-SCALE / (self.sample_rate * time_s)).exp()
    }

    pub fn reset(&mut self) {
        for e in &mut self.env {
            e.reset(1.0);
        }
    }
}
