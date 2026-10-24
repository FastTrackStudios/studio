//! `ReverseDelay` — input-triggered reversed playback windows.
//!
//! Records forward continuously; two alternating grain windows (each
//! `time_ms` long, half a cycle apart) play the buffer backwards with a
//! raised-cosine crossfade. The window cycle re-syncs to input onsets —
//! the `TimeLine` MX "reverse process is synced to performance" behavior —
//! so a phrase played after silence reverses from its own start instead
//! of landing at a random point in a free-running cycle.
//!
//! A reversed read must walk backward through absolute time while the
//! write head walks forward, so the read offset advances **two** samples
//! per tick (`offset = 2·pos + 1`). Advancing it by one — the previous
//! implementation — pins the read to a single frozen sample.

use crate::modulation::Diffuser;
use crate::tilt::DecayTilt;
use dsp_core::audio::biquad::{Biquad, FilterType};
use dsp_core::audio::delay_line::DelayLine;
use dsp_core::num;

/// Reverse delay using onset-synced alternating reversed grains.
pub struct ReverseDelay {
    /// Delay time in milliseconds (grain length).
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// Crossfade overlap as fraction of grain length (0.0–0.5).
    pub grain_crossfade: f64,
    /// High-cut filter frequency in Hz (0 = disabled).
    pub hicut_freq: f64,
    /// Filter Q.
    pub filter_q: f64,
    /// Decay EQ tilt (-1.0 = darken repeats, 0 = neutral, +1.0 = brighten).
    pub decay_tilt: f64,
    /// Smear (0.0–1.0): allpass diffusion softening the reversed
    /// attacks / enhancing the swell (`TimeLine` "Smear" 0–18).
    pub smear: f64,
    /// Delay-line modulation LFO rate in Hz.
    pub mod_rate_hz: f64,
    /// Delay-line modulation depth (0.0–1.0; full scale ≈ ±4 ms).
    pub mod_depth: f64,

    decay_tilt_eq: DecayTilt,
    delay: DelayLine,
    hicut: Biquad,
    diffuser: Diffuser,
    feedback_sample: f64,
    sample_rate: f64,
    /// Current position within the grain cycle (`0..2·grain_samples`).
    grain_pos: usize,
    /// Length of current grain in samples.
    grain_samples: usize,
    // Onset detector: envelope follower with hysteresis re-arm.
    env: f64,
    env_attack: f64,
    env_release: f64,
    armed: bool,
    lfo_phase: f64,
}

impl Default for ReverseDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl ReverseDelay {
    const MAX_DELAY_S: f64 = 5.0;
    /// Onset threshold (≈ −34 dBFS) and re-arm threshold (≈ −46 dBFS).
    const ONSET_ON: f64 = 0.02;
    const ONSET_OFF: f64 = 0.005;
    /// Full-depth modulation excursion in seconds (≈ ±4 ms).
    const MOD_RANGE_S: f64 = 0.004;

    #[must_use]
    pub fn new() -> Self {
        Self {
            time_ms: 250.0,
            feedback: 0.4,
            grain_crossfade: 0.1,
            hicut_freq: 0.0,
            filter_q: 0.707,
            decay_tilt: 0.0,
            smear: 0.0,
            mod_rate_hz: 0.8,
            mod_depth: 0.0,
            decay_tilt_eq: DecayTilt::new(),
            delay: DelayLine::new(48000 * 5 + 1024),
            hicut: Biquad::new(),
            diffuser: Diffuser::new(48000.0, false),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            grain_pos: 0,
            grain_samples: 12000, // 250ms at 48kHz
            env: 0.0,
            env_attack: 0.04,
            env_release: 0.0003,
            armed: true,
            lfo_phase: 0.0,
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

        self.decay_tilt_eq.configure(self.decay_tilt, sample_rate);

        // Onset follower: ~0.5 ms attack, ~80 ms release. The release
        // sets how much silence re-arms the trigger between phrases.
        self.env_attack = 1.0 - (-1.0 / (0.0005 * sample_rate)).exp();
        self.env_release = 1.0 - (-1.0 / (0.08 * sample_rate)).exp();

        // Smear: fixed-size allpass chain, diffusion amount from the knob.
        // Coefficient capped at 0.5 — an 8-stage allpass cascade with high
        // per-stage g rings hard enough to build peaks inside the
        // recirculation loop.
        self.diffuser.size = 0.35;
        self.diffuser.smear = self.smear.clamp(0.0, 1.0) * 0.5;
        self.diffuser.update(sample_rate, false);

        self.grain_samples = num::f64_to_index(self.time_ms * 0.001 * sample_rate).max(64);
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let grain_len = num::f64_to_index(self.time_ms * 0.001 * self.sample_rate).max(64);
        self.grain_samples = grain_len;

        // ── Onset detection: re-sync the window cycle to the attack ──
        let level = input.abs();
        let coeff = if level > self.env {
            self.env_attack
        } else {
            self.env_release
        };
        self.env += (level - self.env) * coeff;
        if self.armed && self.env > Self::ONSET_ON {
            self.grain_pos = 0;
            self.armed = false;
        } else if !self.armed && self.env < Self::ONSET_OFF {
            self.armed = true;
        }

        // Record forward (input + recirculated reversed repeats).
        self.delay.write(input + self.feedback_sample);

        // Delay-line modulation (wobble on the reversed read heads).
        let mod_off = if self.mod_depth > 0.0 {
            self.lfo_phase += self.mod_rate_hz / self.sample_rate;
            if self.lfo_phase >= 1.0 {
                self.lfo_phase -= 1.0;
            }
            (self.lfo_phase * core::f64::consts::TAU).sin()
                * self.mod_depth
                * Self::MOD_RANGE_S
                * self.sample_rate
        } else {
            0.0
        };

        // Two reversed read heads, half a cycle apart.
        let pos_a = self.grain_pos;
        let cycle = grain_len.saturating_mul(2);
        let pos_b = self
            .grain_pos
            .saturating_add(grain_len)
            .checked_rem(cycle)
            .unwrap_or(0);

        let read_a = self.read_reversed(pos_a, grain_len, mod_off);
        let read_b = self.read_reversed(pos_b, grain_len, mod_off);

        // Crossfade windows
        let cf = self.grain_crossfade.clamp(0.01, 0.5);
        let win_a = Self::grain_window(pos_a, grain_len, cf);
        let win_b = Self::grain_window(pos_b, grain_len, cf);

        let mut output = read_a * win_a + read_b * win_b;

        // Smear: diffuse the reversed audio (softened attack, more swell).
        if self.smear > 0.001 {
            output = self.diffuser.tick(output).clamp(-4.0, 4.0);
        }

        // Advance position
        self.grain_pos = self
            .grain_pos
            .saturating_add(1)
            .checked_rem(grain_len.saturating_mul(2))
            .unwrap_or(0);

        // Feedback path
        let mut fb = output * self.feedback;
        if self.hicut_freq > 0.0 {
            fb = self.hicut.tick(fb, ch);
        }

        fb = self.decay_tilt_eq.tick(fb, ch);

        fb = fb.clamp(-1.5, 1.5);
        self.feedback_sample = fb;

        output
    }

    /// Read a reversed grain from the delay line.
    ///
    /// `pos` is the position within the grain cycle (`0..2·grain_len`).
    /// The offset advances 2 samples per tick, so the absolute read
    /// position walks backward at −1× through the grain recorded just
    /// before this window started.
    #[inline]
    fn read_reversed(&self, pos: usize, grain_len: usize, mod_off: f64) -> f64 {
        let pos_in_grain = num::count_to_f64(pos.checked_rem(grain_len).unwrap_or(0));
        let read_offset = 2.0f64.mul_add(pos_in_grain, 1.0) + mod_off;
        let max_read = num::count_to_f64(self.delay.len().saturating_sub(4));
        self.delay.read_cubic(read_offset.clamp(1.0, max_read))
    }

    /// Raised cosine window for a grain.
    /// `cf` is the crossfade fraction (0.01–0.5).
    #[inline]
    fn grain_window(pos: usize, grain_len: usize, cf: f64) -> f64 {
        let pos_in_grain = pos.checked_rem(grain_len).unwrap_or(0);
        let fade_samples = num::f64_to_index(num::count_to_f64(grain_len) * cf);
        let fade_samples = fade_samples.max(1);

        if pos_in_grain < fade_samples {
            // Fade in: raised cosine
            let t = num::count_to_f64(pos_in_grain) / num::count_to_f64(fade_samples);
            0.5 * (1.0 - (core::f64::consts::PI * t).cos())
        } else if pos_in_grain >= grain_len.saturating_sub(fade_samples) {
            // Fade out: raised cosine
            let t = num::count_to_f64(grain_len.saturating_sub(1).saturating_sub(pos_in_grain))
                / num::count_to_f64(fade_samples);
            0.5 * (1.0 - (core::f64::consts::PI * t).cos())
        } else {
            1.0
        }
    }

    #[must_use]
    pub const fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.hicut.reset();
        self.decay_tilt_eq.reset();
        self.diffuser.reset();
        self.feedback_sample = 0.0;
        self.grain_pos = 0;
        self.env = 0.0;
        self.armed = true;
        self.lfo_phase = 0.0;
    }
}
