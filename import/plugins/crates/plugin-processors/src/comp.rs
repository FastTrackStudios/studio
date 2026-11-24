//! The compressor: `dsp::comp`'s generic model.

use dsp::comp::{CompressorConfig, CompressorProcessor, GenericControls, Model, ProcessSpec};
use dsp_core::num::narrow;

use crate::param::{ParamSpec, Unit};
use crate::{BlockProcessor, BuildError, ModSourceSpec};

pub const PARAMS: &[ParamSpec] = &[
    ParamSpec::new("threshold", "Threshold", Unit::Db, -60.0, 0.0, -18.0),
    ParamSpec::new("ratio", "Ratio", Unit::Plain(1), 1.0, 20.0, 4.0),
    ParamSpec::new("attack", "Attack", Unit::Ms, 0.1, 200.0, 10.0),
    ParamSpec::new("release", "Release", Unit::Ms, 5.0, 2000.0, 100.0),
    ParamSpec::new("makeup", "Makeup", Unit::Db, 0.0, 24.0, 0.0),
    ParamSpec::new("mix", "Mix", Unit::Percent, 0.0, 1.0, 1.0),
];

pub const SOURCES: &[ModSourceSpec] = &[ModSourceSpec {
    id: "gain_reduction",
    name: "Gain Reduction",
    unit: Unit::Db,
}];

pub struct Comp {
    controls: GenericControls,
    mix: f64,
    spec: ProcessSpec,
    proc: CompressorProcessor,
}

impl Comp {
    /// # Errors
    ///
    /// When the dsp crate refuses the sample rate or block size.
    pub fn new(sample_rate: f32, max_block: usize) -> Result<Self, BuildError> {
        let failed = |e: dsp::comp::Error| BuildError(alloc::format!("compressor: {e}"));
        let spec = ProcessSpec::new(f64::from(sample_rate), max_block).map_err(failed)?;
        let controls = GenericControls::default();
        let prepared = CompressorConfig::new(Model::Generic(controls))
            .prepare(spec)
            .map_err(failed)?;
        Ok(Self {
            controls,
            mix: 1.0,
            spec,
            proc: prepared.processor(),
        })
    }

    fn apply(&mut self) {
        let mut config = CompressorConfig::new(Model::Generic(self.controls));
        config.mix = self.mix;
        // Same model and spec: the processor keeps its envelope history.
        if let Ok(prepared) = config.prepare(self.spec) {
            let _ = self.proc.apply(&prepared);
        }
    }
}

impl BlockProcessor for Comp {
    fn params(&self) -> &'static [ParamSpec] {
        PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let v = f64::from(value);
        match index {
            0 => self.controls.threshold_db = v,
            1 => self.controls.ratio = v,
            2 => self.controls.attack_ms = v,
            3 => self.controls.release_ms = v,
            4 => self.controls.makeup_db = v,
            5 => self.mix = v,
            _ => return,
        }
        self.apply();
    }

    fn reset(&mut self) {
        self.proc.reset();
    }

    fn mod_sources(&self) -> &'static [ModSourceSpec] {
        SOURCES
    }

    fn mod_source(&self, index: usize) -> f32 {
        match index {
            0 => {
                let [a, b] = self.proc.gain_reduction_db();
                narrow(a.max(b))
            }
            _ => 0.0,
        }
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let [ol, or] = self.proc.process_frame([f64::from(*l), f64::from(*r)]);
            *l = narrow(ol);
            *r = narrow(or);
        }
    }
}
