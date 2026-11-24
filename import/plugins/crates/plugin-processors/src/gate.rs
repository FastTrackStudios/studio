//! The drum gate: `dsp::gate::DrumGate`.
//!
//! Keyed from its own input, or from a key handed in before a piece
//! ([`BlockProcessor::set_key`]): in a kit, the piece's `Trig` channel, so
//! the gate opens on the drum and not on the bleed.

use dsp::gate::{DrumGate, Mode, Settings};

use crate::param::{ParamSpec, Unit};
use crate::{BlockProcessor, ModSourceSpec};

const MODES: &[&str] = &["Kick", "Snare Top", "Snare Bottom", "Toms"];

pub const PARAMS: &[ParamSpec] = &[
    ParamSpec::new("threshold", "Threshold", Unit::Db, -70.0, 0.0, -30.0),
    ParamSpec::new("reduction", "Reduction", Unit::Db, -80.0, 0.0, -80.0),
    ParamSpec::new("length", "Length", Unit::Ms, 50.0, 2000.0, 300.0),
    ParamSpec::choice("mode", "Mode", MODES),
];

pub const SOURCES: &[ModSourceSpec] = &[ModSourceSpec {
    id: "gate",
    name: "Gate",
    unit: Unit::Percent,
}];

pub struct Gate {
    gate: DrumGate,
    settings: Settings,
    sample_rate: f64,
    /// The key for the next piece (mono-summed), and how much of it there is.
    key: alloc::vec::Vec<f64>,
    keyed: usize,
}

impl Gate {
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        Self::with_max_block(sample_rate, 1024)
    }

    /// A gate whose key can be up to `max_block` frames a piece.
    #[must_use]
    pub fn with_max_block(sample_rate: f32, max_block: usize) -> Self {
        let settings = Settings::default();
        let sr = f64::from(sample_rate);
        Self {
            gate: DrumGate::new(sr, 2, settings),
            settings,
            sample_rate: sr,
            key: alloc::vec![0.0; max_block],
            keyed: 0,
        }
    }
}

impl BlockProcessor for Gate {
    fn params(&self) -> &'static [ParamSpec] {
        PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let v = f64::from(value);
        match index {
            0 => self.settings.threshold_db = v,
            1 => self.settings.reduction_db = v,
            2 => self.settings.length_ms = v,
            3 => {
                self.settings.mode = match dsp_core::num::f32_to_index(value) {
                    1 => Mode::SnareTop,
                    2 => Mode::SnareBottom,
                    3 => Mode::Toms,
                    _ => Mode::Kick,
                };
            }
            _ => return,
        }
        self.gate.set_settings(self.settings);
    }

    fn reset(&mut self) {
        self.gate.reset();
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let keyed = core::mem::take(&mut self.keyed);
        for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
            let mut frame = [f64::from(*l), f64::from(*r)];
            match self.key.get(i).filter(|_| i < keyed) {
                // The key on both channels: the detector takes the louder.
                Some(k) => self.gate.process_frame(&mut frame, Some(&[*k, *k])),
                None => self.gate.process_frame(&mut frame, None),
            }
            *l = dsp_core::num::narrow(frame[0]);
            *r = dsp_core::num::narrow(frame[1]);
        }
    }

    fn set_key(&mut self, left: &[f32], right: &[f32]) {
        let n = left.len().min(right.len()).min(self.key.len());
        for ((k, l), r) in self.key.iter_mut().zip(left).zip(right).take(n) {
            *k = (f64::from(*l) + f64::from(*r)) * 0.5;
        }
        self.keyed = n;
    }

    fn mod_sources(&self) -> &'static [ModSourceSpec] {
        SOURCES
    }

    fn mod_source(&self, index: usize) -> f32 {
        if index == 0 {
            dsp_core::num::narrow(self.gate.gain())
        } else {
            0.0
        }
    }

    fn latency(&self) -> u32 {
        u32::try_from(DrumGate::latency_for(self.settings.mode, self.sample_rate)).unwrap_or(0)
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn a_keyed_gate_follows_its_key_not_its_input() {
        let mut g = Gate::with_max_block(48_000.0, 4096);
        // Loud input (bleed), a silent key: the gate stays shut.
        let quiet = alloc::vec![0.0_f32; 4096];
        let mut l = alloc::vec![0.5_f32; 4096];
        let mut r = l.clone();
        for _ in 0..4 {
            l.fill(0.5);
            r.fill(0.5);
            g.set_key(&quiet, &quiet);
            g.process(&mut l, &mut r);
        }
        let shut = l.iter().map(|s| s.abs()).fold(0.0, f32::max);
        // The same input, keyed by itself: it opens.
        let mut g = Gate::with_max_block(48_000.0, 4096);
        for _ in 0..4 {
            l.fill(0.5);
            r.fill(0.5);
            let key = l.clone();
            g.set_key(&key, &key);
            g.process(&mut l, &mut r);
        }
        let open = l.iter().map(|s| s.abs()).fold(0.0, f32::max);
        assert!(open > shut * 4.0, "open {open}, shut {shut}");
    }
}
