//! A stereo clean delay: `dsp::delay::clean_delay`.

use dsp::delay::clean_delay::CleanDelay;
use dsp_core::num::narrow;

use crate::BlockProcessor;
use crate::param::{ParamSpec, Unit};

pub const PARAMS: &[ParamSpec] = &[
    ParamSpec::new("time", "Time", Unit::Ms, 1.0, 2000.0, 375.0),
    ParamSpec::new("feedback", "Feedback", Unit::Percent, 0.0, 0.95, 0.35),
    ParamSpec::new("mix", "Mix", Unit::Percent, 0.0, 1.0, 0.25),
    ParamSpec::new("high_cut", "High Cut", Unit::Hz, 1000.0, 20_000.0, 8000.0),
];

/// A stereo clean delay: one line per side.
pub struct Delay {
    lines: [CleanDelay; 2],
    mix: f32,
    sample_rate: f64,
}

impl Delay {
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let sr = f64::from(sample_rate);
        let mut lines = [CleanDelay::new(), CleanDelay::new()];
        // Sizes the lines for this rate: the only allocation, here.
        lines.iter_mut().for_each(|d| d.update(sr));
        Self {
            lines,
            mix: 0.25,
            sample_rate: sr,
        }
    }
}

impl BlockProcessor for Delay {
    fn params(&self) -> &'static [ParamSpec] {
        PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let v = f64::from(value);
        for d in &mut self.lines {
            match index {
                0 => d.time_ms = v,
                1 => d.feedback = v,
                3 => d.hicut_freq = v,
                _ => {}
            }
            // The lines are already this rate's length: no allocation.
            d.update(self.sample_rate);
        }
        if index == 2 {
            self.mix = value;
        }
    }

    fn reset(&mut self) {
        self.lines.iter_mut().for_each(CleanDelay::reset);
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let [dl, dr] = &mut self.lines;
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let wl = narrow(dl.tick(f64::from(*l), 0));
            let wr = narrow(dr.tick(f64::from(*r), 0));
            *l += self.mix * wl;
            *r += self.mix * wr;
        }
    }
}
