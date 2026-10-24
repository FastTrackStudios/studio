//! Polyphonic Octave Generator — Filter Bank + Envelope Resynthesis.
//!
//! Architecture matching the EHX POG / Micro POG:
//! 1. Log-spaced IIR bandpass filter bank splits the input
//! 2. Per-band envelope follower extracts amplitude
//! 3. Per-band oscillator generates a clean tone at the target octave
//! 4. Oscillator amplitude is modulated by the envelope
//! 5. All bands are summed
//!
//! This produces the characteristic "organ-like" polyphonic octave tone:
//! clean sine-based output with the input's amplitude envelope.
//!
//! Latency: 0 samples (IIR filters only).
//! Character: Organ-like, polyphonic, zero latency.

use std::f64::consts::TAU;

use dsp_core::audio::biquad::{Biquad, FilterType};

/// Number of bandpass filter bands.
const NUM_BANDS: usize = 64;

/// Lowest band center frequency (Hz).
const FREQ_LO: f64 = 27.5;

/// Highest band center frequency (Hz).
const FREQ_HI: f64 = 14000.0;

/// Quality factor for bandpass filters.
const BAND_Q: f64 = 5.0;

/// Which octave shift to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OctaveShift {
    /// Two octaves down (−24 semitones).
    Sub2,
    /// One octave down (−12 semitones).
    Sub1,
    /// One octave up (+12 semitones).
    Up1,
    /// Two octaves up (+24 semitones).
    Up2,
}

impl OctaveShift {
    /// Map a semitone value to the nearest octave shift.
    pub fn from_semitones(st: f64) -> Self {
        if st <= -18.0 {
            Self::Sub2
        } else if st < 0.0 {
            Self::Sub1
        } else if st < 18.0 {
            Self::Up1
        } else {
            Self::Up2
        }
    }

    /// Frequency ratio for this shift.
    fn ratio(self) -> f64 {
        match self {
            Self::Sub2 => 0.25,
            Self::Sub1 => 0.5,
            Self::Up1 => 2.0,
            Self::Up2 => 4.0,
        }
    }
}

// ── Per-Band State ─────────────────────────────────────────────────────

/// Per-band state: bandpass filter + envelope follower + oscillator.
struct Band {
    /// Bandpass filter (2nd-order IIR).
    bp: Biquad,
    /// Center frequency (Hz).
    fc: f64,

    // ── Envelope follower ──
    /// Peak-tracked envelope value.
    env: f64,
    /// Attack coefficient (fast, ~1 ms).
    env_attack: f64,
    /// Release coefficient (slower, ~10 ms).
    env_release: f64,
    /// Second-stage smoothed envelope (removes 2f ripple from rectification).
    env_smooth: f64,
    /// Smoothing coefficient: 1-pole LPF at fc/4 to attenuate 2f ripple by ~18 dB.
    smooth_coeff: f64,

    /// Frequency-dependent gain (higher bands get lower gain for natural spectral tilt).
    gain: f64,

    // ── Output oscillator ──
    /// Oscillator phase accumulator.
    osc_phase: f64,
    /// Phase increment per sample (precomputed).
    osc_phase_inc: f64,
}

impl Band {
    fn new(fc: f64, q: f64, target_freq: f64, sample_rate: f64) -> Self {
        let mut bp = Biquad::new();
        bp.set(FilterType::Bandpass, fc, q, sample_rate);

        // Envelope follower coefficients.
        let attack_ms = 1.0;
        let release_ms = 10.0;
        let env_attack = (-TAU / (attack_ms * 0.001 * sample_rate)).exp();
        let env_release = (-TAU / (release_ms * 0.001 * sample_rate)).exp();

        // Smoothing LPF at fc/4: attenuates 2f ripple by ~18 dB uniformly across bands.
        let smooth_coeff = (-TAU * (fc / 4.0) / sample_rate).exp();

        // Spectral tilt: -3 dB/octave relative to 200 Hz reference.
        // Models the natural 1/f spectral slope of musical signals.
        let gain = (200.0 / fc).powf(0.15);

        Self {
            bp,
            fc,
            env: 0.0,
            env_attack,
            env_release,
            env_smooth: 0.0,
            smooth_coeff,
            gain,
            osc_phase: 0.0,
            osc_phase_inc: TAU * target_freq / sample_rate,
        }
    }

    /// Update oscillator target frequency.
    fn set_target_freq(&mut self, target_freq: f64, sample_rate: f64) {
        self.osc_phase_inc = TAU * target_freq / sample_rate;
    }

    fn reset(&mut self) {
        self.bp.reset();
        self.env = 0.0;
        self.env_smooth = 0.0;
        self.osc_phase = 0.0;
    }

    /// Process one sample: bandpass → envelope → modulated oscillator.
    #[inline]
    fn tick(&mut self, input: f64) -> f64 {
        // Bandpass filter.
        let filtered = self.bp.tick(input, 0);

        // Envelope follower (peak detection with asymmetric smoothing).
        let abs_val = filtered.abs();
        let coeff = if abs_val > self.env {
            self.env_attack
        } else {
            self.env_release
        };
        self.env = coeff * self.env + (1.0 - coeff) * abs_val;

        // Second-stage smoothing: removes 2f ripple from rectification.
        self.env_smooth =
            self.smooth_coeff * self.env_smooth + (1.0 - self.smooth_coeff) * self.env;

        // Triangle wave: odd harmonics at 1/n² amplitude — moderate
        // harmonic content between sine (none) and sawtooth (1/n all).
        let phase_norm = self.osc_phase / TAU;
        let osc = if phase_norm < 0.25 {
            4.0 * phase_norm
        } else if phase_norm < 0.75 {
            2.0 - 4.0 * phase_norm
        } else {
            -4.0 + 4.0 * phase_norm
        };

        self.osc_phase += self.osc_phase_inc;
        if self.osc_phase >= TAU {
            self.osc_phase -= TAU;
        }

        // Output: triangle at target freq × smoothed envelope × spectral tilt.
        osc * self.env_smooth * self.gain
    }
}

// ── PolyOctave (public API) ────────────────────────────────────────────

/// Polyphonic Octave Generator using filter bank + envelope resynthesis.
pub struct PolyOctave {
    /// Which octave shift to apply.
    pub shift: OctaveShift,
    /// Dry/wet mix: 0.0 = dry, 1.0 = wet.
    pub mix: f64,

    /// Log-spaced bandpass filter bank with per-band envelope + oscillator.
    bands: Vec<Band>,
    /// Output highpass to remove DC and sub-bass artifacts.
    hp: Biquad,

    sample_rate: f64,
    last_shift: OctaveShift,
}

impl PolyOctave {
    pub fn new() -> Self {
        let sr = 48000.0;
        let shift = OctaveShift::Sub1;
        let mut s = Self {
            shift,
            mix: 1.0,
            bands: Vec::new(),
            hp: Biquad::new(),
            sample_rate: sr,
            last_shift: shift,
        };
        s.init_bands();
        s.update_hp();
        s
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.init_bands();
        self.update_hp();
        self.reset();
    }

    pub fn reset(&mut self) {
        for band in &mut self.bands {
            band.reset();
        }
        self.hp.reset();
    }

    /// Process one sample. Returns the mixed output.
    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        if self.shift != self.last_shift {
            self.reconfigure();
        }

        let nyquist = self.sample_rate * 0.5;
        let ratio = self.shift.ratio();

        let mut wet = 0.0;
        for band in &mut self.bands {
            // Skip bands whose target frequency would alias.
            if band.fc * ratio >= nyquist {
                continue;
            }
            wet += band.tick(input);
        }

        // Highpass to remove DC artifacts.
        wet = self.hp.tick(wet, 0);

        // Per-shift gain compensation. Two-octave shifts need more gain
        // because the filter bank covers a narrower effective range.
        let gain = match self.shift {
            OctaveShift::Sub2 => 0.70,
            OctaveShift::Sub1 => 0.70,
            OctaveShift::Up1 => 0.60,
            OctaveShift::Up2 => 0.85,
        };
        wet *= gain;

        input * (1.0 - self.mix) + wet * self.mix
    }

    pub fn latency(&self) -> usize {
        0
    }

    /// Initialize the filter bank with log-spaced bands.
    fn init_bands(&mut self) {
        self.bands.clear();
        let log_lo = FREQ_LO.ln();
        let log_hi = FREQ_HI.ln();
        let ratio = self.shift.ratio();
        for i in 0..NUM_BANDS {
            let t = i as f64 / (NUM_BANDS - 1) as f64;
            let fc = (log_lo + t * (log_hi - log_lo)).exp();
            let target = fc * ratio;
            self.bands
                .push(Band::new(fc, BAND_Q, target, self.sample_rate));
        }
    }

    /// Reconfigure oscillator frequencies when shift mode changes.
    fn reconfigure(&mut self) {
        let ratio = self.shift.ratio();
        let sr = self.sample_rate;
        for band in &mut self.bands {
            band.set_target_freq(band.fc * ratio, sr);
        }
        self.update_hp();
        self.last_shift = self.shift;
    }

    /// Configure the output highpass filter.
    fn update_hp(&mut self) {
        // Gentle highpass to remove DC only. Keep very low to preserve sub content.
        let cutoff = match self.shift {
            OctaveShift::Sub2 => 10.0,
            OctaveShift::Sub1 => 12.0,
            OctaveShift::Up1 => 15.0,
            OctaveShift::Up2 => 20.0,
        };
        self.hp
            .set(FilterType::Highpass, cutoff, 0.707, self.sample_rate);
    }
}

impl Default for PolyOctave {
    fn default() -> Self {
        Self::new()
    }
}
