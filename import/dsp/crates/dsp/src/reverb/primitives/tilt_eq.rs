//! Tilt EQ — single control rotates spectrum around a pivot frequency.
//!
//! Implemented as complementary RBJ low/high shelves (Q = 0.707) from the
//! shared `dsp_core::audio::Biquad`: negative tilt darkens (boost lows, cut
//! highs), positive brightens. Each shelf carries half the tilt so the
//! response pivots symmetrically around `pivot_hz`.

use dsp_core::audio::biquad::{Biquad, FilterType};

const SHELF_Q: f64 = 0.707;

pub struct TiltEq {
    low_shelf: Biquad,
    high_shelf: Biquad,
    sample_rate: f64,
    pivot_hz: f64,
    tilt_db: f64,
}

impl TiltEq {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let mut t = Self {
            low_shelf: Biquad::new(),
            high_shelf: Biquad::new(),
            sample_rate,
            pivot_hz: 700.0,
            tilt_db: 0.0,
        };
        t.update();
        t
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sample_rate = sr;
        self.update();
    }

    pub fn set_pivot(&mut self, hz: f64) {
        self.pivot_hz = hz.clamp(20.0, 20000.0);
        self.update();
    }

    /// Negative = darker, positive = brighter. ±12 dB is the practical range.
    pub fn set_tilt_db(&mut self, db: f64) {
        self.tilt_db = db.clamp(-24.0, 24.0);
        self.update();
    }

    fn update(&mut self) {
        // Each shelf contributes half the tilt at its band edge.
        let low_db = -self.tilt_db * 0.5;
        let high_db = self.tilt_db * 0.5;

        self.low_shelf.set(
            FilterType::LowShelf { gain_db: low_db },
            self.pivot_hz,
            SHELF_Q,
            self.sample_rate,
        );
        self.high_shelf.set(
            FilterType::HighShelf { gain_db: high_db },
            self.pivot_hz,
            SHELF_Q,
            self.sample_rate,
        );
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        // Each TiltEq instance is mono — use channel 0 of the stereo biquads.
        self.high_shelf.tick(self.low_shelf.tick(input, 0), 0)
    }

    pub fn reset(&mut self) {
        self.low_shelf.reset();
        self.high_shelf.reset();
    }
}
