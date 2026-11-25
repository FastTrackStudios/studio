//! Granular pitch shifter — fixed-ratio time-domain grain overlap.
//!
//! Two read heads sweep through a circular buffer at `speed` rate relative
//! to the write head. Grains are crossfaded with complementary Hann windows
//! that sum to exactly 1.0 at all times. No pitch detection required — works
//! on any signal.
//!
//! Latency: `grain_size` samples (default 1024).
//! Character: Natural, slight chorus-like artifacts at extreme shifts.

use dsp_core::audio::delay_line::DelayLine;

/// Granular pitch shifter with dual crossfading grains.
pub struct GranularShifter {
    /// Pitch ratio: 0.5 = octave down, 2.0 = octave up.
    pub speed: f64,
    /// Mix: 0.0 = dry only, 1.0 = wet only.
    pub mix: f64,
    /// Grain size in samples.
    pub grain_size: usize,

    delay: DelayLine,
    /// Read offset for grain A (samples behind write head).
    offset_a: f64,
    /// Read offset for grain B (offset by half a grain).
    offset_b: f64,
    /// Phase within the current grain cycle (0.0–1.0).
    grain_phase: f64,
    /// Phase increment per sample.
    phase_inc: f64,

    sample_rate: f64,
}

impl GranularShifter {
    const MAX_BUF_S: f64 = 1.0;

    pub fn new() -> Self {
        let buf_len = 48000 + 4096;
        Self {
            speed: 0.5,
            mix: 1.0,
            grain_size: 1024,
            delay: DelayLine::new(buf_len),
            offset_a: 1024.0,
            offset_b: 1024.0 + 512.0,
            grain_phase: 0.0,
            phase_inc: 1.0 / 1024.0,
            sample_rate: 48000.0,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let max_len = (sample_rate * Self::MAX_BUF_S) as usize + 4096;
        if self.delay.len() < max_len {
            self.delay = DelayLine::new(max_len);
        }
        self.phase_inc = 1.0 / self.grain_size as f64;
        self.offset_a = self.grain_size as f64;
        self.offset_b = self.grain_size as f64 + self.grain_size as f64 * 0.5;
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.offset_a = self.grain_size as f64;
        self.offset_b = self.grain_size as f64 + self.grain_size as f64 * 0.5;
        self.grain_phase = 0.0;
    }

    /// Raised-cosine crossfade: `sin²(π * phase/2)` over [0, 1].
    /// Two windows offset by 0.5 sum to exactly 1.0 everywhere:
    ///   sin²(x) + sin²(x + π/2) = sin²(x) + cos²(x) = 1
    #[inline]
    fn crossfade_window(phase: f64) -> f64 {
        let s = (std::f64::consts::PI * phase).sin();
        s * s
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        self.delay.write(input);

        let max_offset = self.delay.len() as f64 - 4.0;
        let drift = 1.0 - self.speed;

        // Advance read heads by drift.
        self.offset_a += drift;
        self.offset_b += drift;

        // Advance grain phase.
        let prev_phase = self.grain_phase;
        self.grain_phase += self.phase_inc;

        // Reset grain A at phase wrap (1.0 → 0.0).
        if self.grain_phase >= 1.0 {
            self.grain_phase -= 1.0;
            // Smoothly reset: snap to the target offset so the next grain
            // starts reading from a consistent position.
            self.offset_a = self.grain_size as f64;
        }

        // Reset grain B at half-phase boundary.
        if prev_phase < 0.5 && self.grain_phase >= 0.5 {
            self.offset_b = self.grain_size as f64;
        }

        // Clamp offsets to valid range (protect against runaway drift).
        self.offset_a = self.offset_a.clamp(1.0, max_offset);
        self.offset_b = self.offset_b.clamp(1.0, max_offset);

        // Read grains with cubic interpolation.
        let a = self.delay.read_cubic(self.offset_a);
        let b = self.delay.read_cubic(self.offset_b);

        // Crossfade: grain A fades in over its first half, grain B is offset by 0.5.
        // Using sin² ensures the two windows always sum to 1.0.
        let win_a = Self::crossfade_window(self.grain_phase);
        let win_b = Self::crossfade_window((self.grain_phase + 0.5).fract());

        let wet = a * win_a + b * win_b;

        input * (1.0 - self.mix) + wet * self.mix
    }

    pub fn latency(&self) -> usize {
        self.grain_size
    }
}

impl Default for GranularShifter {
    fn default() -> Self {
        Self::new()
    }
}
