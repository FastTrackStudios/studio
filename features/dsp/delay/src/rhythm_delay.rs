//! `RhythmDelay` — multi-tap rhythm delay with 8 taps at integer multiples.
//!
//! Each tap reads at 1x, 2x, 3x… 8x the base delay time, with independent
//! per-tap level control and a decay tilt EQ in the feedback path for tonal
//! shaping of repeats.

use crate::tilt::DecayTilt;
use dsp_core::audio::biquad::{Biquad, FilterType};
use dsp_core::audio::delay_line::DelayLine;
use dsp_core::audio::smoothing::ParamSmoother;
use dsp_core::num;

/// Multi-tap rhythm delay — 8 taps at integer multiples of base time.
pub struct RhythmDelay {
    /// Base delay time in milliseconds.
    pub time_ms: f64,
    /// Feedback amount (clamped to -1.5..1.5 internally).
    pub feedback: f64,
    /// Per-tap levels (0.0–1.0). Tap i reads at (i+1) * `time_ms`.
    pub tap_levels: [f64; 8],
    /// High-cut filter frequency in Hz (0 = disabled).
    pub hicut_freq: f64,
    /// Low-cut filter frequency in Hz (0 = disabled).
    pub locut_freq: f64,
    /// Filter Q.
    pub filter_q: f64,
    /// Decay tilt (-1.0 to 1.0). Negative = darkening repeats, positive = brightening.
    pub decay_tilt: f64,

    delay: DelayLine,
    hicut: Biquad,
    locut: Biquad,
    decay_tilt_eq: DecayTilt,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
}

impl Default for RhythmDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl RhythmDelay {
    /// 5 seconds * 8 taps = 40 seconds max delay.
    const MAX_DELAY_S: f64 = 40.0;

    #[must_use]
    pub fn new() -> Self {
        Self {
            time_ms: 250.0,
            feedback: 0.4,
            tap_levels: [1.0, 0.7, 0.5, 0.35, 0.25, 0.18, 0.12, 0.08],
            hicut_freq: 0.0,
            locut_freq: 0.0,
            filter_q: 0.707,
            decay_tilt: 0.0,
            delay: DelayLine::new(48000 * 40 + 1024),
            hicut: Biquad::new(),
            locut: Biquad::new(),
            decay_tilt_eq: DecayTilt::new(),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let max_len = num::f64_to_index(sample_rate * Self::MAX_DELAY_S).saturating_add(1024);
        if self.delay.len() < max_len {
            self.delay = DelayLine::new(max_len);
        }

        if self.hicut_freq > 0.0 {
            self.hicut.set(
                FilterType::Lowpass,
                self.hicut_freq,
                self.filter_q,
                sample_rate,
            );
        }
        if self.locut_freq > 0.0 {
            self.locut.set(
                FilterType::Highpass,
                self.locut_freq,
                self.filter_q,
                sample_rate,
            );
        }

        self.decay_tilt_eq.configure(self.decay_tilt, sample_rate);

        self.smoother
            .set_time_seeded(0.15, sample_rate, self.time_ms * 0.001 * sample_rate);
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();

        let max_read = num::count_to_f64(self.delay.len()) - 4.0;

        // Sum all 8 taps
        let mut output = 0.0;
        let mut last_tap = 0.0;
        for (i, &level) in self.tap_levels.iter().enumerate() {
            let tap_delay = smooth_delay * num::count_to_f64(i.saturating_add(1));
            let read_pos = tap_delay.clamp(1.0, max_read);
            let tap_out = self.delay.read_cubic(read_pos);
            output += tap_out * level;
            if i == 7 {
                last_tap = tap_out;
            }
        }

        // Feedback path: last tap (8x) → filters → clamp
        let mut fb = last_tap * self.feedback;

        if self.hicut_freq > 0.0 {
            fb = self.hicut.tick(fb, ch);
        }
        if self.locut_freq > 0.0 {
            fb = self.locut.tick(fb, ch);
        }
        fb = self.decay_tilt_eq.tick(fb, ch);

        fb = fb.clamp(-1.5, 1.5);

        self.delay.write(input + fb);
        self.feedback_sample = fb;

        output
    }

    #[must_use]
    pub const fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.hicut.reset();
        self.locut.reset();
        self.decay_tilt_eq.reset();
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
    }
}
