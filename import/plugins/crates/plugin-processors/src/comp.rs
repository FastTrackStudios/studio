//! The compressor: `dsp::comp`'s generic model, in `dsp::comp::CompChain`
//! for its detector's high-pass (so a kick doesn't pump a bus) and its
//! lookahead (so, at 20:1 and the fastest attack, it holds peaks: a
//! limiter).

use dsp::comp::{CompChain, CompressorConfig, GenericControls, Model, ProcessSpec};
use dsp_core::audio::AudioConfig;
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
    // The detector only: below this the compressor doesn't listen. 20 is off.
    ParamSpec::new("sc_hpf", "Sidechain HPF", Unit::Hz, 20.0, 500.0, 20.0),
    // The audio this late, the detector on time.
    ParamSpec::new(
        "lookahead",
        "Lookahead",
        Unit::Ms,
        0.0,
        MAX_LOOKAHEAD_MS as f32,
        0.0,
    ),
];

/// The most lookahead, reserved when the block is built.
const MAX_LOOKAHEAD_MS: f64 = 10.0;

pub const SOURCES: &[ModSourceSpec] = &[ModSourceSpec {
    id: "gain_reduction",
    name: "Gain Reduction",
    unit: Unit::Db,
}];

pub struct Comp {
    controls: GenericControls,
    mix: f64,
    spec: ProcessSpec,
    chain: CompChain,
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
        let mut chain = CompChain::new();
        chain.update(AudioConfig {
            sample_rate: f64::from(sample_rate),
            max_buffer_size: max_block,
        });
        chain.reserve_lookahead(MAX_LOOKAHEAD_MS);
        chain.set_model(&prepared);
        // On the model from the first sample, not crossfading into it.
        chain.reset();
        Ok(Self {
            controls,
            mix: 1.0,
            spec,
            chain,
        })
    }

    fn apply(&mut self) {
        let mut config = CompressorConfig::new(Model::Generic(self.controls));
        config.mix = self.mix;
        // Same model and spec: the processor keeps its envelope history.
        if let Ok(prepared) = config.prepare(self.spec) {
            self.chain.set_model(&prepared);
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
            6 => {
                self.chain.set_sidechain_freq(v);
                return;
            }
            7 => {
                self.chain.set_lookahead(v.clamp(0.0, MAX_LOOKAHEAD_MS));
                return;
            }
            _ => return,
        }
        self.apply();
    }

    fn reset(&mut self) {
        self.chain.reset();
    }

    fn latency(&self) -> u32 {
        u32::try_from(self.chain.lookahead_samples).unwrap_or(0)
    }

    fn mod_sources(&self) -> &'static [ModSourceSpec] {
        SOURCES
    }

    fn mod_source(&self, index: usize) -> f32 {
        match index {
            0 => narrow(self.chain.gain_reduction_db()),
            _ => 0.0,
        }
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let (mut a, mut b) = (f64::from(*l), f64::from(*r));
            self.chain.process_sample(&mut a, &mut b);
            *l = narrow(a);
            *r = narrow(b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak_after(comp: &mut Comp, x: &[f32]) -> f32 {
        let (mut l, mut r) = (x.to_vec(), x.to_vec());
        comp.process(&mut l, &mut r);
        l.iter().skip(x.len() / 2).fold(0.0, |m, s| m.max(s.abs()))
    }

    /// Clicks over a quiet bed, as a snare over a kit.
    fn clicks(n: usize) -> alloc::vec::Vec<f32> {
        (0..n)
            .map(|i| {
                if i % 4800 < 2 {
                    0.9
                } else {
                    0.05 * libm::sinf(i as f32 * 0.05)
                }
            })
            .collect()
    }

    #[test]
    fn with_lookahead_it_holds_peaks_and_says_how_late() {
        let x = clicks(48_000);
        let set = |c: &mut Comp, look: f32| {
            for (i, v) in [(0, -20.0), (1, 20.0), (2, 0.1), (3, 50.0), (7, look)] {
                c.set_param(i, v);
            }
            c.reset();
        };
        let mut plain = Comp::new(48_000.0, 48_000).unwrap();
        set(&mut plain, 0.0);
        let mut ahead = Comp::new(48_000.0, 48_000).unwrap();
        set(&mut ahead, 1.0);
        assert_eq!(plain.latency(), 0);
        assert_eq!(ahead.latency(), 48);
        let (p, a) = (peak_after(&mut plain, &x), peak_after(&mut ahead, &x));
        // -20 dBFS at 20:1: the clicks held near 0.1, not let through.
        assert!(a < p * 0.6, "lookahead {a} vs none {p}");
        assert!(a < 0.2, "{a}");
    }

    #[test]
    fn its_detector_high_pass_ignores_the_lows() {
        // A loud 40 Hz under a quiet 2 kHz: with the detector high-passed at
        // 200 Hz, the lows don't pull the level down.
        let x: alloc::vec::Vec<f32> = (0..48_000)
            .map(|i| {
                let t = i as f32 / 48_000.0;
                0.8 * libm::sinf(t * 40.0 * core::f32::consts::TAU)
                    + 0.05 * libm::sinf(t * 2000.0 * core::f32::consts::TAU)
            })
            .collect();
        let gr = |hpf: f32| {
            let mut c = Comp::new(48_000.0, 48_000).unwrap();
            for (i, v) in [(0, -30.0), (1, 8.0), (6, hpf)] {
                c.set_param(i, v);
            }
            c.reset();
            let (mut l, mut r) = (x.clone(), x.clone());
            c.process(&mut l, &mut r);
            c.mod_source(0)
        };
        let (full, filtered) = (gr(20.0), gr(200.0));
        assert!(full > 10.0, "the lows drive it: {full} dB");
        assert!(
            filtered < full * 0.5,
            "high-passed: {filtered} dB vs {full} dB"
        );
    }
}
