//! A drive stage: `dsp::saturate::preamp::ClassAPreamp`.

use dsp::saturate::preamp::{ClassAPreamp, SideShaper};

use crate::BlockProcessor;
use crate::param::{ParamSpec, Unit};

const SHAPES: &[&str] = &["Clean", "Op-Amp", "Tube", "Transformer", "Diode", "Hard"];

pub const PARAMS: &[ParamSpec] = &[
    ParamSpec::new("drive", "Drive", Unit::Db, 0.0, 24.0, 6.0),
    ParamSpec::choice("shape", "Shape", SHAPES),
    ParamSpec::new("bias", "Bias", Unit::Plain(2), -1.0, 1.0, 0.0),
    ParamSpec::new("mix", "Mix", Unit::Percent, 0.0, 1.0, 1.0),
    ParamSpec::new("level", "Level", Unit::Db, -24.0, 12.0, 0.0),
];

pub struct Drive {
    pre: ClassAPreamp,
}

impl Drive {
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let mut pre = ClassAPreamp::new(sample_rate);
        pre.positive = SideShaper::Tube;
        pre.negative = SideShaper::Tube;
        pre.drive = crate::db_to_gain(6.0);
        pre.refresh_makeup();
        Self { pre }
    }
}

impl BlockProcessor for Drive {
    fn params(&self) -> &'static [ParamSpec] {
        PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.pre.drive = crate::db_to_gain(value),
            1 => {
                let shape = SideShaper::from_index(
                    u32::try_from(dsp_core::num::f32_to_index(value)).unwrap_or(0),
                );
                self.pre.positive = shape;
                self.pre.negative = shape;
            }
            2 => self.pre.q_point = value,
            3 => self.pre.mix = value,
            4 => self.pre.output_gain = crate::db_to_gain(value),
            _ => return,
        }
        // A setter-path refresh (a handful of arithmetic, no allocation).
        self.pre.refresh_makeup();
    }

    fn reset(&mut self) {
        self.pre.reset();
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            *l = self.pre.process(0, *l);
            *r = self.pre.process(1, *r);
        }
    }
}
