//! Spectral fingerprint matching for bleed rejection.
//!
//! After an onset is detected, a short FFT snapshot is taken and compared
//! against a learned template to determine if the hit is the target drum
//! or bleed from another source. Inspired by XLN Addictive Trigger's
//! "Audio Fingerprint" technology.
//!
//! Workflow:
//! 1. **Learn phase**: User plays the target drum, system captures
//!    spectral snapshots and builds an average template.
//! 2. **Match phase**: Each detected onset's spectrum is compared
//!    against the template using normalized cross-correlation.
//!    Hits below the similarity threshold are rejected as bleed.
//!
//! The spectral fingerprint captures the timbral character of a drum
//! hit — kick drums have energy concentrated below 200Hz, snares have
//! a mid-frequency peak plus high-frequency wire rattle, etc.

use rustfft::{FftPlanner, num_complex::Complex};

/// Spectral fingerprint for a drum sound.
#[derive(Clone)]
pub struct SpectralFingerprint {
    /// Normalized magnitude spectrum (L2-normalized).
    pub spectrum: Vec<f64>,
    /// Number of snapshots averaged into this template.
    pub num_snapshots: usize,
    /// FFT size used to create this fingerprint.
    pub fft_size: usize,
}

/// Spectral fingerprint matcher for bleed rejection.
pub struct FingerprintMatcher {
    /// Learned template fingerprint.
    template: Option<SpectralFingerprint>,

    /// FFT size for fingerprint computation.
    fft_size: usize,

    /// Hanning window.
    window: Vec<f64>,

    /// Similarity threshold (0.0-1.0). Hits below this are rejected.
    /// Higher = stricter matching. 0.7 is a good starting point.
    pub threshold: f64,

    /// Whether the matcher is in learn mode.
    pub learning: bool,

    /// Accumulator for learning (sum of normalized spectra).
    learn_accum: Vec<f64>,
    learn_count: usize,

    _sample_rate: f64,
}

impl FingerprintMatcher {
    #[must_use]
    pub fn new(fft_size: usize, sample_rate: f64) -> Self {
        let window: Vec<f64> = (0..fft_size)
            .map(|i| 0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / fft_size as f64).cos()))
            .collect();

        let num_bins = fft_size / 2 + 1;

        Self {
            template: None,
            fft_size,
            window,
            threshold: 0.7,
            learning: false,
            learn_accum: vec![0.0; num_bins],
            learn_count: 0,
            _sample_rate: sample_rate,
        }
    }

    /// Start learning mode. Call `learn_hit()` for each example hit.
    pub fn start_learning(&mut self) {
        self.learning = true;
        self.learn_accum.fill(0.0);
        self.learn_count = 0;
    }

    /// Feed a hit's audio (centered around the transient) for learning.
    /// The buffer should be at least `fft_size` samples.
    pub fn learn_hit(&mut self, audio: &[f64]) {
        if audio.len() < self.fft_size {
            return;
        }

        let spectrum = self.compute_spectrum(audio);
        let norm = Self::normalize(&spectrum);

        for (acc, &val) in self.learn_accum.iter_mut().zip(norm.iter()) {
            *acc += val;
        }
        self.learn_count += 1;
    }

    /// Finish learning and create the template fingerprint.
    /// Returns the number of snapshots used.
    pub fn finish_learning(&mut self) -> usize {
        self.learning = false;

        if self.learn_count == 0 {
            self.template = None;
            return 0;
        }

        let mut avg: Vec<f64> = self
            .learn_accum
            .iter()
            .map(|&v| v / self.learn_count as f64)
            .collect();

        // Re-normalize the average
        let norm = Self::l2_norm(&avg);
        if norm > 0.0 {
            for v in &mut avg {
                *v /= norm;
            }
        }

        self.template = Some(SpectralFingerprint {
            spectrum: avg,
            num_snapshots: self.learn_count,
            fft_size: self.fft_size,
        });

        self.learn_count
    }

    /// Set a pre-computed template fingerprint.
    pub fn set_template(&mut self, template: SpectralFingerprint) {
        self.template = Some(template);
    }

    /// Check if a template has been learned.
    #[must_use]
    pub const fn has_template(&self) -> bool {
        self.template.is_some()
    }

    /// Match a detected hit's audio against the template.
    ///
    /// Returns `Some(similarity)` where similarity is 0.0-1.0
    /// (normalized cross-correlation). Returns `None` if no template.
    #[must_use]
    pub fn match_hit(&self, audio: &[f64]) -> Option<f64> {
        let template = self.template.as_ref()?;

        if audio.len() < self.fft_size {
            return Some(0.0);
        }

        let spectrum = self.compute_spectrum(audio);
        let norm = Self::normalize(&spectrum);

        // Normalized cross-correlation (dot product of L2-normalized vectors)
        let similarity: f64 = norm
            .iter()
            .zip(template.spectrum.iter())
            .map(|(&a, &b)| a * b)
            .sum();

        Some(similarity.clamp(0.0, 1.0))
    }

    /// Check if a hit matches the template above the threshold.
    #[must_use]
    pub fn is_match(&self, audio: &[f64]) -> bool {
        match self.match_hit(audio) {
            Some(sim) => sim >= self.threshold,
            None => true, // No template = accept all
        }
    }

    /// Compute magnitude spectrum of an audio buffer.
    fn compute_spectrum(&self, audio: &[f64]) -> Vec<f64> {
        let mut fft_buf: Vec<Complex<f64>> = (0..self.fft_size)
            .map(|i| Complex::new(audio[i] * self.window[i], 0.0))
            .collect();

        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(self.fft_size);
        fft.process(&mut fft_buf);

        let num_bins = self.fft_size / 2 + 1;
        (0..num_bins).map(|k| fft_buf[k].norm()).collect()
    }

    /// L2-normalize a spectrum.
    fn normalize(spectrum: &[f64]) -> Vec<f64> {
        let norm = Self::l2_norm(spectrum);
        if norm > 0.0 {
            spectrum.iter().map(|&v| v / norm).collect()
        } else {
            vec![0.0; spectrum.len()]
        }
    }

    fn l2_norm(v: &[f64]) -> f64 {
        v.iter().map(|x| x * x).sum::<f64>().sqrt()
    }

    /// Reset (clear template and learning state).
    pub fn reset(&mut self) {
        self.template = None;
        self.learning = false;
        self.learn_accum.fill(0.0);
        self.learn_count = 0;
    }
}
