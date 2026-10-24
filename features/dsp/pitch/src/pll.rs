//! Phase-locked loop tracking oscillator for sub-octave generation.
//!
//! Tracks the input pitch via a PLL and generates a sub-oscillator that
//! follows at half (or quarter) frequency. Supports saw, triangle, and
//! square waveforms for warmer tones than a simple frequency divider.
//!
//! Latency: 0 samples.
//! Character: Warm, analog-like. Smoother than pure frequency division.

use serde::{Deserialize, Serialize};

/// Sub-oscillator waveform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubWaveform {
    Square,
    Saw,
    Triangle,
}

/// Octave division for the PLL sub-oscillator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PllOctave {
    /// One octave down (÷2).
    Oct1,
    /// Two octaves down (÷4).
    Oct2,
}

/// Phase-locked loop sub-oscillator.
pub struct PllTracker {
    /// Sub-oscillator waveform.
    pub waveform: SubWaveform,
    /// Octave division.
    pub octave: PllOctave,
    /// Mix: 0.0 = dry only, 1.0 = wet only.
    pub mix: f64,

    // PLL state.
    /// Phase accumulator for the VCO (0.0–1.0).
    vco_phase: f64,
    /// Current estimated frequency (Hz).
    vco_freq: f64,
    /// Phase detector integrator (proportional).
    pd_prop: f64,
    /// Phase detector integrator (integral).
    pd_int: f64,

    // Input phase tracking.
    prev_sample: f64,
    was_positive: bool,
    /// Samples since last zero crossing (for frequency estimation).
    samples_since_zc: usize,
    /// Last measured half-period in samples.
    last_half_period: usize,

    // Envelope follower.
    envelope: f64,
    attack_coeff: f64,
    release_coeff: f64,

    // DC blocker.
    dc_x1: f64,
    dc_y1: f64,
    dc_coeff: f64,

    sample_rate: f64,
    hysteresis: f64,

    // PLL bandwidth (controls tracking speed vs. stability).
    /// Proportional gain.
    kp: f64,
    /// Integral gain.
    ki: f64,
}

impl PllTracker {
    pub fn new() -> Self {
        Self {
            waveform: SubWaveform::Saw,
            octave: PllOctave::Oct1,
            mix: 1.0,
            vco_phase: 0.0,
            vco_freq: 100.0,
            pd_prop: 0.0,
            pd_int: 0.0,
            prev_sample: 0.0,
            was_positive: false,
            samples_since_zc: 0,
            last_half_period: 480,
            envelope: 0.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            dc_x1: 0.0,
            dc_y1: 0.0,
            dc_coeff: 0.997,
            sample_rate: 48000.0,
            hysteresis: 0.01,
            kp: 0.02,
            ki: 0.0001,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.attack_coeff = (-1.0 / (0.0005 * sample_rate)).exp();
        self.release_coeff = (-1.0 / (0.01 * sample_rate)).exp();
        self.dc_coeff = 1.0 - (std::f64::consts::TAU * 10.0 / sample_rate);
    }

    pub fn reset(&mut self) {
        self.vco_phase = 0.0;
        self.vco_freq = 100.0;
        self.pd_prop = 0.0;
        self.pd_int = 0.0;
        self.prev_sample = 0.0;
        self.was_positive = false;
        self.samples_since_zc = 0;
        self.last_half_period = 480;
        self.envelope = 0.0;
        self.dc_x1 = 0.0;
        self.dc_y1 = 0.0;
    }

    /// Process one sample. Returns mixed output.
    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        // Envelope follower.
        let abs_in = input.abs();
        let coeff = if abs_in > self.envelope {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.envelope = coeff * self.envelope + (1.0 - coeff) * abs_in;

        self.samples_since_zc += 1;

        // Zero-crossing detection with hysteresis.
        let is_positive = if self.was_positive {
            input > -self.hysteresis
        } else {
            input > self.hysteresis
        };

        if is_positive != self.was_positive {
            // Update frequency estimate from half-period.
            if self.samples_since_zc > 2 {
                self.last_half_period = self.samples_since_zc;
                let measured_freq = self.sample_rate / (2.0 * self.last_half_period as f64);

                // Clamp to reasonable guitar range (20–2000 Hz).
                let clamped = measured_freq.clamp(20.0, 2000.0);

                // PLL phase detector: compare measured vs VCO frequency.
                let error = clamped - self.vco_freq;
                self.pd_prop = error * self.kp;
                self.pd_int += error * self.ki;
                self.pd_int = self.pd_int.clamp(-500.0, 500.0);

                self.vco_freq = (self.vco_freq + self.pd_prop + self.pd_int).clamp(20.0, 2000.0);
            }
            self.samples_since_zc = 0;
            self.was_positive = is_positive;
        }

        // Advance VCO phase at sub-octave frequency.
        let divisor = match self.octave {
            PllOctave::Oct1 => 2.0,
            PllOctave::Oct2 => 4.0,
        };
        let sub_freq = self.vco_freq / divisor;
        self.vco_phase += sub_freq / self.sample_rate;
        self.vco_phase -= self.vco_phase.floor();

        // Generate waveform from phase.
        let sub_raw = match self.waveform {
            SubWaveform::Square => {
                if self.vco_phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            SubWaveform::Saw => 2.0 * self.vco_phase - 1.0,
            SubWaveform::Triangle => {
                if self.vco_phase < 0.5 {
                    4.0 * self.vco_phase - 1.0
                } else {
                    3.0 - 4.0 * self.vco_phase
                }
            }
        };

        // Shape by envelope.
        let sub_shaped = sub_raw * self.envelope;

        // DC blocker.
        let dc_out = sub_shaped - self.dc_x1 + self.dc_coeff * self.dc_y1;
        self.dc_x1 = sub_shaped;
        self.dc_y1 = dc_out;

        // Mix.
        input * (1.0 - self.mix) + dc_out * self.mix
    }

    pub fn latency(&self) -> usize {
        0
    }

    /// Current PLL frequency estimate in Hz (the tracked input pitch,
    /// before octave division). Useful for driving external
    /// synthesis from the tracker.
    pub fn frequency(&self) -> f64 {
        self.vco_freq
    }

    /// Current input envelope (linear, follows |input|).
    pub fn envelope_value(&self) -> f64 {
        self.envelope
    }
}

impl Default for PllTracker {
    fn default() -> Self {
        Self::new()
    }
}
