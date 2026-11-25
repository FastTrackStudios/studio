//! Spectral tilt around 1 kHz.
//!
//! Adds `log2(f / 1000) * slope` dB to every bin, so the displayed spectrum is
//! tilted around the 1 kHz pivot. A 4.5 dB/oct tilt roughly compensates for the
//! natural slope of music, flattening typical program material. Port of
//! `ZLEqualizer`'s `SpectrumTilter`.

pub struct SpectrumTilter {
    shift_db: Vec<f32>,
}

impl SpectrumTilter {
    #[must_use]
    pub fn new(num_bins: usize) -> Self {
        Self {
            shift_db: vec![0.0; num_bins],
        }
    }

    pub fn resize(&mut self, num_bins: usize) {
        self.shift_db.resize(num_bins, 0.0);
    }

    /// Recompute the per-bin shift for the given sample rate and slope (dB/oct).
    pub fn set_slope(&mut self, sample_rate: f64, slope_per_oct: f64) {
        let n = self.shift_db.len();
        if n < 2 {
            return;
        }
        let delta = sample_rate * 0.5 / (n - 1) as f64;
        for i in 1..n {
            let freq = i as f64 * delta;
            self.shift_db[i] = (freq / 1000.0).log2() as f32 * slope_per_oct as f32;
        }
        self.shift_db[0] = self.shift_db[1];
    }

    /// Apply the tilt to a dB spectrum in place.
    pub fn tilt(&self, spectrum_db: &mut [f32]) {
        for (s, shift) in spectrum_db.iter_mut().zip(self.shift_db.iter()) {
            *s += *shift;
        }
    }
}
