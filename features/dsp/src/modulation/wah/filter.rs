//! Wah filter — Chamberlin state-variable filter with LP/BP/HP mixing.
//!
//! Uses a cascadable SVF topology for the characteristic vocal-like sweep.
//! The output is a configurable mix of lowpass, bandpass, and highpass,
//! allowing classic wah, mutron, and phaser-like tones.
//!
//! Techniques from: `ZynAddSubFX` (SVF topology), rkrlv2/RyanWah (mix mode,
//! variable Q tracking, exponential frequency mapping).

use std::f64::consts::PI;

/// Wah filter mode — controls the LP/BP/HP mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WahMode {
    /// Classic wah — bandpass-dominant with some lowpass bleed.
    Classic,
    /// Mutron — bandpass-only, sharp vocal resonance.
    Mutron,
    /// Low-pass wah — darker sweep.
    Lowpass,
    /// Phase-like — mixed HP/BP for phaser-ish response.
    Phaser,
}

impl WahMode {
    /// LP, BP, HP mix weights for this mode.
    const fn mix_weights(&self) -> (f64, f64, f64) {
        match self {
            Self::Classic => (0.2, 0.8, 0.0),
            Self::Mutron => (0.0, 1.0, 0.0),
            Self::Lowpass => (1.0, 0.2, 0.0),
            Self::Phaser => (0.0, 0.5, 0.5),
        }
    }
}

/// Chamberlin SVF state for one channel.
#[derive(Clone, Default)]
struct SvfState {
    low: f64,
    band: f64,
    high: f64,
}

/// Chamberlin state-variable filter with LP/BP/HP mixing.
///
/// Produces simultaneous lowpass, bandpass, and highpass outputs.
/// The wah tone is a weighted mix of these three.
///
/// Supports cascaded stages for steeper slopes.
pub struct WahFilter {
    /// Filter mode (controls LP/BP/HP mix).
    pub mode: WahMode,
    /// Base Q / resonance (1.0–20.0). Higher = more vocal.
    pub q: f64,
    /// Number of cascaded filter stages (1–4).
    pub stages: usize,
    /// Variable Q tracking (0..1). When > 0, Q varies inversely with
    /// the modulation position — emulates real inductor wah pedals.
    pub variq: f64,

    states: [[SvfState; 4]; 2], // [channel][stage]
    current_freq: f64,
    f_coeff: f64,
    q_coeff: f64,
    sample_rate: f64,
}

impl WahFilter {
    /// Minimum sweep frequency (Hz).
    pub const MIN_FREQ: f64 = 200.0;
    /// Maximum sweep frequency (Hz).
    pub const MAX_FREQ: f64 = 5000.0;
    /// Exponential mapping base (from rkrlv2).
    const FREQ_BASE: f64 = 7.0;

    #[must_use]
    pub fn new() -> Self {
        Self {
            mode: WahMode::Classic,
            q: 3.0,
            stages: 1,
            variq: 0.0,
            states: Default::default(),
            current_freq: 800.0,
            f_coeff: 0.0,
            q_coeff: 0.0,
            sample_rate: 48000.0,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.set_freq(self.current_freq);
    }

    /// Set the filter cutoff frequency directly.
    pub fn set_freq(&mut self, freq: f64) {
        self.current_freq = freq.clamp(Self::MIN_FREQ, Self::MAX_FREQ);
        // Chamberlin SVF coefficient: f = 2 * sin(pi * fc / fs)
        self.f_coeff = (2.0 * (PI * self.current_freq / self.sample_rate).sin()).clamp(0.0, 0.95); // stability clamp
        self.update_q(self.q);
    }

    /// Set cutoff from a normalized position (0..1).
    ///
    /// Uses exponential mapping (base 7.0) for perceptually uniform sweep.
    pub fn set_position(&mut self, pos: f64) {
        let pos = pos.clamp(0.0, 1.0);
        let freq =
            Self::MIN_FREQ + Self::MAX_FREQ * (Self::FREQ_BASE.powf(pos) - 1.0) / Self::FREQ_BASE;
        self.set_freq(freq);
    }

    /// Set position with variable Q tracking.
    ///
    /// When `variq > 0`, Q varies inversely with position — higher position
    /// (brighter) gets lower Q, lower position (darker) gets higher Q.
    /// This emulates the behavior of real inductor-based wah pedals.
    pub fn set_position_with_env(&mut self, pos: f64, env_level: f64) {
        self.set_position(pos);
        if self.variq > 0.0 {
            // Q varies inversely with envelope: louder = less resonant
            let q_scale = 2.0f64.mul_add(1.0 - env_level, 1.0).exp2();
            let varied_q = self.q.mul_add(1.0 - self.variq, q_scale * self.variq);
            self.update_q(varied_q);
        }
    }

    fn update_q(&mut self, q: f64) {
        // HiQ mode: direct inverse for better sound (per rkrlv2)
        self.q_coeff = (1.0 / q.max(0.5)).clamp(0.01, 1.99);
    }

    /// Process one sample through the cascaded SVF.
    #[inline]
    pub fn tick(&mut self, sample: f64, ch: usize) -> f64 {
        let ch = ch.min(1);
        let (lp_gain, bp_gain, hp_gain) = self.mode.mix_weights();
        let n = self.stages.clamp(1, 4);

        let mut smp = sample;

        for stage in 0..n {
            let s = &mut self.states[ch][stage];

            // Chamberlin SVF topology
            s.low = dsp_core::audio::denormal::flush(self.f_coeff.mul_add(s.band, s.low));
            s.high = self.q_coeff.mul_add(-s.band, smp - s.low);
            s.band = dsp_core::audio::denormal::flush(self.f_coeff.mul_add(s.high, s.band));

            // Mix LP/BP/HP
            smp = hp_gain.mul_add(s.high, lp_gain.mul_add(s.low, bp_gain * s.band));
        }

        smp
    }

    /// Get the current cutoff frequency.
    #[must_use]
    pub const fn freq(&self) -> f64 {
        self.current_freq
    }

    pub fn reset(&mut self) {
        self.states = Default::default();
    }
}

impl Default for WahFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Triple-cascaded envelope smoother.
///
/// Three first-order lowpass smoothers in series create a 3rd-order
/// characteristic with natural, lag-free response.
/// Based on rkrlv2's `RyanWah` envelope smoothing (20ms time constant).
pub struct TripleSmoother {
    s1: f64,
    s2: f64,
    s3: f64,
    coeff: f64,
}

impl TripleSmoother {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            s1: 0.0,
            s2: 0.0,
            s3: 0.0,
            coeff: 0.01,
        }
    }

    /// Set the smoothing time constant.
    pub fn set_time_ms(&mut self, ms: f64, sample_rate: f64) {
        let time_s = ms * 0.001;
        if time_s > 0.0 && sample_rate > 0.0 {
            self.coeff = 1.0 - (-1.0 / (time_s * sample_rate)).exp();
        } else {
            self.coeff = 1.0;
        }
    }

    /// Process one sample through three cascaded smoothers.
    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        self.s1 += (input - self.s1) * self.coeff;
        self.s2 += (self.s1 - self.s2) * self.coeff;
        self.s3 += (self.s2 - self.s3) * self.coeff;
        self.s3
    }

    #[must_use]
    pub const fn value(&self) -> f64 {
        self.s3
    }

    pub const fn reset(&mut self, val: f64) {
        self.s1 = val;
        self.s2 = val;
        self.s3 = val;
    }
}

impl Default for TripleSmoother {
    fn default() -> Self {
        Self::new()
    }
}
