//! Unison engine — audio-domain voice multiplication.
//!
//! The one engine behind FTS-Unison (double-tracking simulation) and
//! signal's synth unison: N voices, each an independent pitch-shifted
//! copy of the input with its own detune, pan, and decorrelation
//! delay, summed around the (optional) dry center.
//!
//! Voice layout follows the classic super-saw convention: detunes
//! spread symmetrically across ±detune, pans spread across ±spread,
//! delays grow per voice so no two voices comb the same way. Two
//! voices at full spread with a haas-range delay IS the studio
//! doubler; more voices thicken toward ensemble.

use crate::pitch::chain::{Algorithm, PitchChain};
use dsp_core::audio::{AudioConfig, Processor};

pub const MAX_VOICES: usize = 8;
/// Per-voice decorrelation delay ceiling (ms) — sized into the rings.
const MAX_DELAY_MS: f64 = 80.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnisonConfig {
    /// Number of unison voices (2..=MAX_VOICES).
    pub voices: usize,
    /// Detune span in cents: voices sit at symmetric fractions of ±this.
    pub detune_cents: f64,
    /// Stereo spread 0..1: 0 = all voices center, 1 = hard-panned ends.
    pub spread: f64,
    /// Base decorrelation delay (ms); voice k gets `base·(1 + k/2)`.
    pub delay_ms: f64,
    /// Dry (center) level 0..1.
    pub dry_level: f64,
    /// Voices level 0..1.
    pub wet_level: f64,
    /// Shifting engine for the voices.
    pub algorithm: Algorithm,
}

impl Default for UnisonConfig {
    fn default() -> Self {
        Self {
            voices: 2,
            detune_cents: 12.0,
            spread: 1.0,
            delay_ms: 18.0,
            dry_level: 1.0,
            wet_level: 0.8,
            algorithm: Algorithm::Psola,
        }
    }
}

struct Voice {
    chain: PitchChain,
    ring: Vec<f64>,
    ring_pos: usize,
    /// Constant-power pan gains.
    gain_l: f64,
    gain_r: f64,
    delay_samples: usize,
    /// Mono voice scratch.
    scratch: Vec<f64>,
    scratch_r: Vec<f64>,
}

pub struct UnisonEngine {
    pub config: UnisonConfig,
    voices: Vec<Voice>,
    sample_rate: f64,
}

impl UnisonEngine {
    pub fn new() -> Self {
        Self {
            config: UnisonConfig::default(),
            voices: Vec::new(),
            sample_rate: 48_000.0,
        }
    }

    /// Allocate voices/rings. Call from setup, never the audio thread.
    pub fn prepare(&mut self, sample_rate: f64, max_block: usize) {
        self.sample_rate = sample_rate.max(1.0);
        let ring_len = ((MAX_DELAY_MS / 1000.0) * self.sample_rate) as usize + 2;
        self.voices = (0..MAX_VOICES)
            .map(|_| {
                let mut chain = PitchChain::new();
                chain.algorithm = self.config.algorithm;
                chain.semitones = 0.0;
                chain.mix = 1.0;
                chain.update(AudioConfig {
                    sample_rate: self.sample_rate,
                    max_buffer_size: max_block.max(1),
                });
                Voice {
                    chain,
                    ring: vec![0.0; ring_len],
                    ring_pos: 0,
                    gain_l: core::f64::consts::FRAC_1_SQRT_2,
                    gain_r: core::f64::consts::FRAC_1_SQRT_2,
                    delay_samples: 0,
                    scratch: vec![0.0; max_block.max(1)],
                    scratch_r: vec![0.0; max_block.max(1)],
                }
            })
            .collect();
        self.update_voicing();
    }

    /// Re-derive per-voice detune/pan/delay from the config. Cheap;
    /// call whenever config changes.
    pub fn update_voicing(&mut self) {
        let n = self.config.voices.clamp(2, MAX_VOICES);
        for (k, v) in self.voices.iter_mut().enumerate().take(n) {
            // Symmetric spread positions in −1..1 (super-saw layout).
            let pos = if n == 1 {
                0.0
            } else {
                2.0 * k as f64 / (n - 1) as f64 - 1.0
            };
            v.chain.algorithm = self.config.algorithm;
            v.chain.semitones = pos * self.config.detune_cents / 100.0;
            // Constant-power pan at `pos·spread`.
            let pan = pos * self.config.spread.clamp(0.0, 1.0);
            let angle = (pan + 1.0) * core::f64::consts::FRAC_PI_4;
            v.gain_l = angle.cos();
            v.gain_r = angle.sin();
            // Decorrelation: later voices wait longer.
            let d_ms = self.config.delay_ms.clamp(0.0, MAX_DELAY_MS) * (1.0 + k as f64 * 0.5);
            v.delay_samples = ((d_ms / 1000.0) * self.sample_rate) as usize;
        }
    }

    pub fn latency(&self) -> usize {
        self.voices.first().map(|v| v.chain.latency()).unwrap_or(0)
    }

    /// Process a stereo block in place: dry·level + Σ voices.
    pub fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        let n_samples = left.len().min(right.len());
        let n_voices = self.config.voices.clamp(2, MAX_VOICES);
        let dry = self.config.dry_level.clamp(0.0, 1.0);
        // Voice gain compensation: constant loudness as voices stack.
        let wet = self.config.wet_level.clamp(0.0, 1.0) / (n_voices as f64).sqrt();

        for v in self.voices.iter_mut().take(n_voices) {
            // Mono feed (center image into each voice's shifter).
            for i in 0..n_samples {
                v.scratch[i] = 0.5 * (left[i] + right[i]);
                v.scratch_r[i] = v.scratch[i];
            }
            v.chain
                .process(&mut v.scratch[..n_samples], &mut v.scratch_r[..n_samples]);
        }
        for i in 0..n_samples {
            let (mut out_l, mut out_r) = (left[i] * dry, right[i] * dry);
            for v in self.voices.iter_mut().take(n_voices) {
                let ring_len = v.ring.len();
                v.ring[v.ring_pos] = v.scratch[i];
                let read = (v.ring_pos + ring_len - v.delay_samples.min(ring_len - 1)) % ring_len;
                let s = v.ring[read];
                v.ring_pos = (v.ring_pos + 1) % ring_len;
                out_l += s * v.gain_l * wet;
                out_r += s * v.gain_r * wet;
            }
            left[i] = out_l;
            right[i] = out_r;
        }
    }

    pub fn reset(&mut self) {
        for v in &mut self.voices {
            v.chain.reset();
            v.ring.fill(0.0);
            v.ring_pos = 0;
        }
    }
}

impl Default for UnisonEngine {
    fn default() -> Self {
        Self::new()
    }
}
