//! A channel EQ: low cut, low shelf, a mid bell, high shelf, high cut, on
//! `dsp_core`'s biquads. The full FTS-EQ engine (24 dynamic bands) comes
//! over from `builtin` when a preset needs more than a channel strip.

use dsp_core::audio::biquad::{Biquad, FilterType};

use crate::BlockProcessor;
use crate::param::{ParamSpec, Unit};

pub const PARAMS: &[ParamSpec] = &[
    ParamSpec::new("low_cut", "Low Cut", Unit::Hz, 20.0, 1000.0, 20.0),
    ParamSpec::new("low_freq", "Low Freq", Unit::Hz, 30.0, 1000.0, 120.0),
    ParamSpec::new("low_gain", "Low Gain", Unit::Db, -18.0, 18.0, 0.0),
    ParamSpec::new("mid_freq", "Mid Freq", Unit::Hz, 100.0, 10_000.0, 1000.0),
    ParamSpec::new("mid_gain", "Mid Gain", Unit::Db, -18.0, 18.0, 0.0),
    ParamSpec::new("mid_q", "Mid Q", Unit::Plain(2), 0.1, 10.0, 0.7),
    ParamSpec::new("high_freq", "High Freq", Unit::Hz, 1000.0, 16_000.0, 6000.0),
    ParamSpec::new("high_gain", "High Gain", Unit::Db, -18.0, 18.0, 0.0),
    ParamSpec::new("high_cut", "High Cut", Unit::Hz, 1000.0, 20_000.0, 20_000.0),
];

const N: usize = 9;
const LOW_CUT_OFF: f32 = 20.0;
const HIGH_CUT_OFF: f32 = 20_000.0;

pub struct Eq {
    values: [f32; N],
    sample_rate: f64,
    /// low cut, low shelf, mid, high shelf, high cut
    bands: [Biquad; 5],
    active: [bool; 5],
}

impl Eq {
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let mut values = [0.0; N];
        for (v, p) in values.iter_mut().zip(PARAMS) {
            *v = p.default;
        }
        let mut eq = Self {
            values,
            sample_rate: f64::from(sample_rate),
            bands: core::array::from_fn(|_| Biquad::new()),
            active: [false; 5],
        };
        eq.design();
        eq
    }

    fn v(&self, i: usize) -> f64 {
        f64::from(self.values.get(i).copied().unwrap_or(0.0))
    }

    fn design(&mut self) {
        let sr = self.sample_rate;
        let nyq = sr * 0.49;
        let specs = [
            (
                FilterType::Highpass,
                self.v(0),
                0.707,
                self.v(0) > f64::from(LOW_CUT_OFF) + 0.5,
            ),
            (
                FilterType::LowShelf { gain_db: self.v(2) },
                self.v(1),
                0.707,
                self.v(2).abs() > 0.01,
            ),
            (
                FilterType::Peak { gain_db: self.v(4) },
                self.v(3),
                self.v(5),
                self.v(4).abs() > 0.01,
            ),
            (
                FilterType::HighShelf { gain_db: self.v(7) },
                self.v(6),
                0.707,
                self.v(7).abs() > 0.01,
            ),
            (
                FilterType::Lowpass,
                self.v(8),
                0.707,
                self.v(8) < f64::from(HIGH_CUT_OFF) - 0.5,
            ),
        ];
        for ((band, active), (ty, freq, q, on)) in
            self.bands.iter_mut().zip(self.active.iter_mut()).zip(specs)
        {
            *active = on;
            if on {
                band.set(ty, freq.min(nyq), q, sr);
            }
        }
    }
}

impl BlockProcessor for Eq {
    fn params(&self) -> &'static [ParamSpec] {
        PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) {
        if let Some(v) = self.values.get_mut(index) {
            *v = value;
            self.design();
        }
    }

    fn reset(&mut self) {
        self.bands.iter_mut().for_each(Biquad::reset);
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (band, on) in self.bands.iter_mut().zip(self.active) {
            if !on {
                continue;
            }
            for (l, r) in left.iter_mut().zip(right.iter_mut()) {
                *l = dsp_core::num::narrow(band.tick(f64::from(*l), 0));
                *r = dsp_core::num::narrow(band.tick(f64::from(*r), 1));
            }
        }
    }
}
