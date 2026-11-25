//! Barberpole pitch shifter — Dattorro/Schroeder style with cubic interpolation.
//!
//! Two read heads sweep through a circular delay buffer at a rate determined
//! by the pitch ratio. Each head's read offset is computed directly from its
//! sweep phase, guaranteeing:
//! 1. Offsets are always within buffer bounds (no out-of-range reads).
//! 2. Resets happen ONLY at zero crossfade weight (no rollover stutter).
//! 3. The two crossfade windows always sum to exactly 1.0.
//!
//! Cubic (Catmull-Rom) interpolation provides sub-sample accuracy without
//! the clicking artifacts of allpass interpolation.
//!
//! Latency: **0 samples** — output is produced immediately.
//! Character: Classic hardware pitch shifter (Eventide H3000, Boss PS-series).

use dsp_core::audio::delay_line::DelayLine;
use std::f64::consts::PI;

const BUFFER_SIZE: usize = 8192;
/// Margin for cubic interpolation (needs 2 samples on each side).
const MARGIN: f64 = 4.0;
/// Maximum sweep range within the buffer.
const SWEEP_LEN: f64 = (BUFFER_SIZE as f64) - MARGIN * 2.0;

pub struct AllpassShifter {
    /// Pitch ratio: 0.5 = octave down, 2.0 = octave up.
    pub speed: f64,
    /// Mix: 0.0 = dry only, 1.0 = wet only.
    pub mix: f64,

    delay: DelayLine,

    /// Sweep phase for head A [0.0, 1.0).
    /// Position AND crossfade weight are both derived from this.
    sweep_a: f64,
    /// Sweep phase for head B [0.0, 1.0). Always ~0.5 ahead of sweep_a.
    sweep_b: f64,

    sample_rate: f64,
}

impl AllpassShifter {
    pub fn new() -> Self {
        Self {
            speed: 0.5,
            mix: 1.0,
            delay: DelayLine::new(BUFFER_SIZE),
            sweep_a: 0.0,
            sweep_b: 0.5,
            sample_rate: 48000.0,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.sweep_a = 0.0;
        self.sweep_b = 0.5;
    }

    /// Crossfade window: sin²(π * phase).
    /// Two windows offset by 0.5 always sum to exactly 1.0.
    #[inline]
    fn crossfade(phase: f64) -> f64 {
        let s = (PI * phase).sin();
        s * s
    }

    /// Compute read offset (samples behind write head) from sweep phase.
    /// For pitch-down (drift > 0): offset increases with phase (sweeps away).
    /// For pitch-up (drift < 0): offset decreases with phase (sweeps toward).
    /// Always returns a value within [MARGIN, MARGIN + SWEEP_LEN].
    #[inline]
    fn phase_to_offset(phase: f64, drift: f64) -> f64 {
        if drift >= 0.0 {
            MARGIN + phase * SWEEP_LEN
        } else {
            MARGIN + (1.0 - phase) * SWEEP_LEN
        }
    }

    /// Process one sample. Returns the mixed (dry/wet) output.
    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        self.delay.write(input);

        let drift = 1.0 - self.speed;
        let abs_drift = drift.abs().max(0.0001);
        let phase_inc = abs_drift / SWEEP_LEN;

        // Advance sweep phases.
        self.sweep_a += phase_inc;
        self.sweep_b += phase_inc;

        // Wrap phases. At wrap point, crossfade weight = sin²(0) = 0,
        // so the head is completely silent. No audible discontinuity.
        if self.sweep_a >= 1.0 {
            self.sweep_a -= 1.0;
        }
        if self.sweep_b >= 1.0 {
            self.sweep_b -= 1.0;
        }

        // Compute read offsets directly from sweep phases.
        // This guarantees offsets are always in [MARGIN, MARGIN + SWEEP_LEN].
        let offset_a = Self::phase_to_offset(self.sweep_a, drift);
        let offset_b = Self::phase_to_offset(self.sweep_b, drift);

        // Read from each head with cubic interpolation.
        let a = self.delay.read_cubic(offset_a);
        let b = self.delay.read_cubic(offset_b);

        // Crossfade derived directly from sweep phase.
        let win_a = Self::crossfade(self.sweep_a);
        let win_b = Self::crossfade(self.sweep_b);

        let wet = a * win_a + b * win_b;

        input * (1.0 - self.mix) + wet * self.mix
    }

    /// Latency in samples. Always zero for this algorithm.
    pub fn latency(&self) -> usize {
        0
    }
}

impl Default for AllpassShifter {
    fn default() -> Self {
        Self::new()
    }
}
