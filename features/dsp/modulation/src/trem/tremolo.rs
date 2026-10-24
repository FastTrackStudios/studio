//! Core tremolo engine — amplitude modulation via modulator output.
//!
//! Supports harmonic tremolo (split into low/high bands, modulate
//! out of phase) and standard tremolo.

use dsp_core::audio::biquad::{Biquad, FilterType};

/// Tremolo shape applied to the raw modulator output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TremShape {
    /// Use the modulator output directly (pattern/LFO shaped).
    Pattern,
    /// Sine wave at the modulator rate.
    Sine,
    /// Triangle wave.
    Triangle,
    /// Square wave with adjustable pulse width.
    Square,
}

/// Tremolo mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TremMode {
    /// Standard tremolo — same modulation on both channels.
    Mono,
    /// Stereo tremolo — L/R modulated with phase offset.
    Stereo,
    /// Harmonic tremolo — low and high bands modulated out of phase.
    Harmonic,
}

/// Single-channel tremolo processor.
///
/// Takes a modulation value (0..1) and applies it as amplitude modulation.
pub struct Tremolo {
    /// Modulation depth (0..1). 0 = no effect, 1 = full depth.
    pub depth: f64,
    /// Tremolo mode.
    pub mode: TremMode,
    /// Crossover frequency for harmonic tremolo (Hz).
    pub crossover_freq: f64,

    // Harmonic tremolo filters
    lp: Biquad,
    hp: Biquad,

    sample_rate: f64,
}

impl Tremolo {
    #[must_use]
    pub fn new() -> Self {
        Self {
            depth: 0.5,
            mode: TremMode::Mono,
            crossover_freq: 800.0,
            lp: Biquad::new(),
            hp: Biquad::new(),
            sample_rate: 48000.0,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.lp
            .set(FilterType::Lowpass, self.crossover_freq, 0.707, sample_rate);
        self.hp.set(
            FilterType::Highpass,
            self.crossover_freq,
            0.707,
            sample_rate,
        );
    }

    /// Apply tremolo to a sample given the modulation value (0..1).
    ///
    /// `mod_val` is the primary modulation, `mod_val_inv` is the inverted
    /// modulation for harmonic tremolo's complementary band.
    #[inline]
    pub fn tick(&mut self, sample: f64, mod_val: f64, ch: usize) -> f64 {
        // Convert 0..1 modulation to gain: at depth=1, gain ranges 0..1
        // at depth=0, gain is always 1 (no modulation)
        let gain = self.depth.mul_add(-(1.0 - mod_val), 1.0);
        let gain_inv = self.depth.mul_add(-mod_val, 1.0);

        match self.mode {
            TremMode::Mono | TremMode::Stereo => sample * gain,
            TremMode::Harmonic => {
                let low = self.lp.tick(sample, ch);
                let high = self.hp.tick(sample, ch);
                low.mul_add(gain, high * gain_inv)
            }
        }
    }

    pub fn reset(&mut self) {
        self.lp.reset();
        self.hp.reset();
    }
}

impl Default for Tremolo {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Analog Style — waveshaper saturation applied after amplitude modulation
// ---------------------------------------------------------------------------

/// Saturation character applied to the tremolo output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnalogStyle {
    /// No saturation — clean digital.
    #[default]
    Clean,
    /// Gentle low-frequency warmth.
    Fat,
    /// Compressed, smooth saturation.
    Squash,
    /// Broadband soft saturation.
    Dirt,
    /// Exaggerated high-end clipping with clean lows.
    Crunch,
    /// Asymmetric hard clipping.
    Shred,
    /// Extreme pumping compression with makeup gain.
    Pump,
}

/// Stateful analog-style processor (Crunch needs a crossover filter).
pub struct AnalogProcessor {
    pub style: AnalogStyle,
    lp: Biquad,
    hp: Biquad,
    sample_rate: f64,
}

impl AnalogProcessor {
    #[must_use]
    pub fn new() -> Self {
        Self {
            style: AnalogStyle::Clean,
            lp: Biquad::new(),
            hp: Biquad::new(),
            sample_rate: 48000.0,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        // Crunch crossover at 2 kHz
        self.lp.set(FilterType::Lowpass, 2000.0, 0.707, sample_rate);
        self.hp
            .set(FilterType::Highpass, 2000.0, 0.707, sample_rate);
    }

    /// Apply the selected analog style to a single sample.
    #[inline]
    pub fn tick(&mut self, x: f64, ch: usize) -> f64 {
        match self.style {
            AnalogStyle::Clean => x,
            AnalogStyle::Fat => {
                // Gentle: tanh(x*1.5) / tanh(1.5)
                let drive = 1.5;
                (x * drive).tanh() / drive.tanh()
            }
            AnalogStyle::Squash => {
                let drive = 2.5;
                (x * drive).tanh() / drive.tanh()
            }
            AnalogStyle::Dirt => {
                let drive = 4.0;
                (x * drive).tanh() / drive.tanh()
            }
            AnalogStyle::Crunch => {
                // Saturate highs, keep lows clean
                let low = self.lp.tick(x, ch);
                let high = self.hp.tick(x, ch);
                let drive = 4.0;
                let sat_high = (high * drive).tanh() / drive.tanh();
                low + sat_high
            }
            AnalogStyle::Shred => {
                // Asymmetric: positive clips harder than negative
                if x >= 0.0 {
                    (x * 6.0).tanh() / 6.0_f64.tanh()
                } else {
                    (x * 3.0).tanh() / 3.0_f64.tanh()
                }
            }
            AnalogStyle::Pump => {
                // Heavy compression with makeup gain
                (x * 8.0).tanh() * 1.5 / 8.0_f64.tanh()
            }
        }
    }

    pub fn reset(&mut self) {
        self.lp.reset();
        self.hp.reset();
    }
}

impl Default for AnalogProcessor {
    fn default() -> Self {
        Self::new()
    }
}
