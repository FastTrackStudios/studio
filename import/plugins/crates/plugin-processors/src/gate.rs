//! The drum gate: `dsp::gate::DrumGate`.
//!
//! Keyed from its own input for now. In a kit the key is the piece's
//! `Trig`, which arrives from another port of the session.

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
}

impl Gate {
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let settings = Settings::default();
        let sr = f64::from(sample_rate);
        Self {
            gate: DrumGate::new(sr, 2, settings),
            settings,
            sample_rate: sr,
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
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let mut frame = [f64::from(*l), f64::from(*r)];
            self.gate.process_frame(&mut frame, None);
            *l = dsp_core::num::narrow(frame[0]);
            *r = dsp_core::num::narrow(frame[1]);
        }
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
