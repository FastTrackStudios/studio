//! The reverb: `dsp::reverb::ReverbChain`.

use alloc::vec;
use alloc::vec::Vec;

use dsp::reverb::{AlgorithmType, ReverbChain};
use dsp_core::audio::{AudioConfig, Processor as _};
use dsp_core::num::narrow;

use crate::BlockProcessor;
use crate::param::{ParamSpec, Unit};

const ALGORITHMS: &[&str] = &["Room", "Hall", "Plate", "Spring"];

pub const PARAMS: &[ParamSpec] = &[
    ParamSpec::choice("algorithm", "Algorithm", ALGORITHMS),
    ParamSpec::new("decay", "Decay", Unit::Percent, 0.0, 1.0, 0.5),
    ParamSpec::new("size", "Size", Unit::Percent, 0.0, 1.0, 0.5),
    ParamSpec::new("damping", "Damping", Unit::Percent, 0.0, 1.0, 0.4),
    ParamSpec::new("predelay", "Pre-delay", Unit::Ms, 0.0, 250.0, 10.0),
    ParamSpec::new("mix", "Mix", Unit::Percent, 0.0, 1.0, 0.2),
];

pub struct Reverb {
    chain: ReverbChain,
    config: AudioConfig,
    algorithm: usize,
    l: Vec<f64>,
    r: Vec<f64>,
}

impl Reverb {
    #[must_use]
    pub fn new(sample_rate: f32, max_block: usize) -> Self {
        let config = AudioConfig {
            sample_rate: f64::from(sample_rate),
            max_buffer_size: max_block,
        };
        let mut chain = ReverbChain::new();
        chain.mix = 0.2;
        chain.update(config);
        Self {
            chain,
            config,
            algorithm: 0,
            l: vec![0.0; max_block],
            r: vec![0.0; max_block],
        }
    }

    const fn algorithm_type(index: usize) -> AlgorithmType {
        match index {
            1 => AlgorithmType::Hall,
            2 => AlgorithmType::Plate,
            3 => AlgorithmType::Spring,
            _ => AlgorithmType::Room,
        }
    }
}

impl BlockProcessor for Reverb {
    fn params(&self) -> &'static [ParamSpec] {
        PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let v = f64::from(value);
        match index {
            0 => {
                let a = dsp_core::num::f32_to_index(value);
                if a != self.algorithm {
                    // A new algorithm builds its engine: this is the one
                    // reverb parameter that allocates, so a graph sets it
                    // when it is built and a live change of it belongs on
                    // the control side (a rebuilt graph), not in a macro.
                    self.algorithm = a;
                    self.chain.set_algorithm(Self::algorithm_type(a));
                    self.chain.update(self.config);
                }
                return;
            }
            1 => self.chain.params.decay = v,
            2 => self.chain.params.size = v,
            3 => self.chain.params.damping = v,
            4 => self.chain.predelay_ms = v,
            5 => self.chain.mix = v,
            _ => return,
        }
        self.chain.update_params();
    }

    fn reset(&mut self) {
        self.chain.reset();
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len()).min(self.l.len());
        let (Some(l), Some(r)) = (self.l.get_mut(..n), self.r.get_mut(..n)) else {
            return;
        };
        for ((d, s), (e, t)) in l
            .iter_mut()
            .zip(left.iter())
            .zip(r.iter_mut().zip(right.iter()))
        {
            *d = f64::from(*s);
            *e = f64::from(*t);
        }
        self.chain.process(l, r);
        for ((d, s), (e, t)) in left
            .iter_mut()
            .zip(l.iter())
            .zip(right.iter_mut().zip(r.iter()))
        {
            *d = narrow(*s);
            *e = narrow(*t);
        }
    }
}
