//! Modulated delay line for pre-delay and late reverb lines.
//!
//! Ported from `CloudSeedCore` ModulatedDelay.h (MIT, Ghost Note Audio).
//! The LFO target is recomputed every 8 samples (as in `CloudSeed`) but the
//! fractional read position ramps per-sample between updates and the buffer
//! is read with cubic interpolation, so fast modulation stays smooth instead
//! of stepping in whole samples.

use dsp_core::num;

use std::f64::consts::PI;

use dsp_core::audio::delay_line::DelayLine;

/// Default capacity: 2 seconds at 48kHz. Resized by `set_sample_rate`.
const DEFAULT_SAMPLE_RATE: f64 = 48000.0;
/// Modulation recalculation rate.
const MOD_UPDATE_RATE: u64 = 8;
/// Buffer length in seconds.
const BUFFER_SECONDS: f64 = 2.0;

pub struct ModulatedDelay {
    buffer: DelayLine,
    samples_processed: u64,
    mod_phase: f64,
    current_delay: f64,
    delay_step: f64,
    pub sample_delay: usize,
    pub mod_amount: f64,
    pub mod_rate: f64,
}

impl ModulatedDelay {
    #[must_use]
    pub fn new() -> Self {
        let mut d = Self {
            buffer: DelayLine::new(num::f64_to_index(DEFAULT_SAMPLE_RATE * BUFFER_SECONDS)),
            samples_processed: 0,
            mod_phase: 0.31,
            current_delay: 100.0,
            delay_step: 0.0,
            sample_delay: 100,
            mod_amount: 0.0,
            mod_rate: 0.0,
        };
        d.update();
        d
    }

    /// Resize the buffer for the actual sample rate. Allocates; call from
    /// setup, never from the audio tick.
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        let len = num::f64_to_index(sample_rate * BUFFER_SECONDS);
        if len > self.buffer.len() {
            self.buffer = DelayLine::new(len);
        }
        self.samples_processed = 0;
        self.current_delay = num::count_to_f64(self.sample_delay);
        self.delay_step = 0.0;
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        if self.samples_processed >= MOD_UPDATE_RATE {
            self.update();
            self.samples_processed = 0;
        }

        // Ramp the read position toward the LFO target computed in update().
        self.current_delay += self.delay_step;

        self.buffer.write(input);
        let max_delay = num::count_to_f64(self.buffer.len().saturating_sub(4));
        let output = self
            .buffer
            .read_cubic(self.current_delay.clamp(1.0, max_delay));

        self.samples_processed = self.samples_processed.saturating_add(1);
        output
    }

    /// Write a sample without producing output. Lets a gated-off caller
    /// keep the buffer warm so re-engaging reads real audio instead of
    /// silence.
    #[inline]
    pub fn write_only(&mut self, input: f64) {
        self.buffer.write(input);
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    pub fn reset(&mut self) {
        self.clear();
        self.samples_processed = 0;
        self.current_delay = num::count_to_f64(self.sample_delay);
        self.delay_step = 0.0;
    }

    fn update(&mut self) {
        self.mod_phase += self.mod_rate * num::u64_to_f64(MOD_UPDATE_RATE);
        if self.mod_phase > 1.0 {
            self.mod_phase %= 1.0;
        }

        let modulation = (self.mod_phase * 2.0 * PI).sin();
        let target = self
            .mod_amount
            .mul_add(modulation, num::count_to_f64(self.sample_delay))
            .max(1.0);

        // Spread the move over the next update window.
        self.delay_step = (target - self.current_delay) / num::u64_to_f64(MOD_UPDATE_RATE);
    }
}

impl Default for ModulatedDelay {
    fn default() -> Self {
        Self::new()
    }
}
