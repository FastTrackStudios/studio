//! Frequency divider — analog-style octave generation (Boss OC-2 / EHX OC-3).
//!
//! Tracks zero crossings of the input waveform and toggles a flip-flop at
//! half the rate to produce a sub-octave square wave. The square wave is
//! shaped by the input's envelope for dynamic tracking.
//!
//! Latency: 0 samples.
//! Character: Synthy, square-wave. The classic "OC-2 sound".

use serde::{Deserialize, Serialize};

/// Octave division ratio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DivideRatio {
    /// One octave down (divide by 2).
    Oct1,
    /// Two octaves down (divide by 4).
    Oct2,
}

/// Analog-style frequency divider for sub-octave generation.
pub struct FreqDivider {
    /// Division ratio.
    pub ratio: DivideRatio,
    /// Mix: 0.0 = dry only, 1.0 = wet only.
    pub mix: f64,

    // Flip-flop state for octave 1 (÷2).
    flip1: bool,
    // Flip-flop state for octave 2 (÷4, driven by flip1 edges).
    flip2: bool,
    flip1_prev: bool,

    // Zero-crossing detection.
    prev_sample: f64,
    // Whether previous sample was positive (for hysteresis).
    was_positive: bool,

    // Envelope follower for shaping the sub output.
    envelope: f64,
    attack_coeff: f64,
    release_coeff: f64,

    // DC blocker (1-pole high-pass) to remove DC from the square wave.
    dc_x1: f64,
    dc_y1: f64,
    dc_coeff: f64,

    // Hysteresis threshold to avoid false triggers from noise.
    hysteresis: f64,
}

impl FreqDivider {
    pub fn new() -> Self {
        Self {
            ratio: DivideRatio::Oct1,
            mix: 1.0,
            flip1: false,
            flip2: false,
            flip1_prev: false,
            prev_sample: 0.0,
            was_positive: false,
            envelope: 0.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            dc_x1: 0.0,
            dc_y1: 0.0,
            dc_coeff: 0.997,
            hysteresis: 0.01,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        // Fast attack (~0.5ms), slower release (~10ms) for envelope.
        self.attack_coeff = (-1.0 / (0.0005 * sample_rate)).exp();
        self.release_coeff = (-1.0 / (0.01 * sample_rate)).exp();
        // DC blocker: ~10 Hz cutoff.
        self.dc_coeff = 1.0 - (std::f64::consts::TAU * 10.0 / sample_rate);
    }

    pub fn reset(&mut self) {
        self.flip1 = false;
        self.flip2 = false;
        self.flip1_prev = false;
        self.prev_sample = 0.0;
        self.was_positive = false;
        self.envelope = 0.0;
        self.dc_x1 = 0.0;
        self.dc_y1 = 0.0;
    }

    /// Process one sample. Returns the mixed output.
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

        // Zero-crossing detection with hysteresis.
        let is_positive = if self.was_positive {
            input > -self.hysteresis
        } else {
            input > self.hysteresis
        };

        // Detect negative→positive zero crossing.
        if is_positive && !self.was_positive {
            self.flip1 = !self.flip1;

            // Octave 2: toggle on flip1 rising edges.
            if self.flip1 && !self.flip1_prev {
                self.flip2 = !self.flip2;
            }
            self.flip1_prev = self.flip1;
        }
        self.was_positive = is_positive;
        self.prev_sample = input;

        // Generate sub signal: square wave shaped by envelope.
        let sub_raw = match self.ratio {
            DivideRatio::Oct1 => {
                if self.flip1 {
                    1.0
                } else {
                    -1.0
                }
            }
            DivideRatio::Oct2 => {
                if self.flip2 {
                    1.0
                } else {
                    -1.0
                }
            }
        };
        let sub_shaped = sub_raw * self.envelope;

        // DC blocker on sub signal.
        let dc_out = sub_shaped - self.dc_x1 + self.dc_coeff * self.dc_y1;
        self.dc_x1 = sub_shaped;
        self.dc_y1 = dc_out;

        // Mix dry/wet.
        input * (1.0 - self.mix) + dc_out * self.mix
    }

    pub fn latency(&self) -> usize {
        0
    }
}

impl Default for FreqDivider {
    fn default() -> Self {
        Self::new()
    }
}
