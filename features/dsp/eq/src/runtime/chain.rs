//! EQ chain managing up to 24 bands using the pro design pipeline.
//!
//! Equivalent to Pro-Q 4's per-channel processing chain that cascades
//! all enabled bands in series.

use crate::runtime::band::Band;

/// Maximum number of bands in a chain (matches Pro-Q 4's 24-band limit).
pub const MAX_BANDS: usize = 24;

/// Complete EQ processing chain.
pub struct EqChain {
    bands: Vec<Band>,
    sample_rate: f64,
    capacity: usize,
}

impl EqChain {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bands: Vec::new(),
            sample_rate: 48000.0,
            capacity: MAX_BANDS,
        }
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            bands: Vec::with_capacity(capacity),
            capacity,
            sample_rate: 48000.0,
        }
    }

    /// Add a band, returning an error without mutation when capacity is full.
    ///
    /// # Errors
    /// Returns an error if configuration or buffer dimensions are invalid.
    pub fn add_band(&mut self) -> Result<usize, crate::Error> {
        if self.bands.len() >= self.capacity {
            return Err(crate::Error::CapacityExceeded);
        }
        let idx = self.bands.len();
        let mut band = Band::new();
        band.update(self.sample_rate);
        self.bands.push(band);
        Ok(idx)
    }

    /// Get a mutable reference to a band by index.
    pub fn band_mut(&mut self, index: usize) -> Option<&mut Band> {
        self.bands.get_mut(index)
    }

    /// Get an immutable reference to a band by index.
    #[must_use]
    pub fn band(&self, index: usize) -> Option<&Band> {
        self.bands.get(index)
    }

    /// Return the number of bands in the chain.
    #[must_use]
    pub const fn num_bands(&self) -> usize {
        self.bands.len()
    }

    /// Recalculate coefficients for a single band.
    pub fn update_band(&mut self, index: usize) {
        if let Some(band) = self.bands.get_mut(index) {
            band.update(self.sample_rate);
        }
    }

    /// Set the sample rate and recalculate all band coefficients.
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        for band in &mut self.bands {
            band.update(sample_rate);
        }
    }

    /// Reset all band processing state to zero.
    pub fn reset(&mut self) {
        for band in &mut self.bands {
            band.reset();
        }
    }

    /// Process interleaved stereo buffers through all bands.
    ///
    /// Each sample passes through all bands in series (left then right).
    /// True when at least one band would touch the signal.
    /// The whole chain's magnitude response at `hz`, in dB.
    #[must_use]
    pub fn magnitude_db(&self, hz: f64, sample_rate: f64) -> f64 {
        self.bands
            .iter()
            .map(|b| b.magnitude_db(hz, sample_rate))
            .sum()
    }

    #[must_use]
    pub fn has_active_bands(&self) -> bool {
        self.bands.iter().any(|b| !b.is_idle())
    }

    pub fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        use crate::runtime::band::Placement;
        // Idle chain: zero per-sample work.
        if !self.has_active_bands() {
            return;
        }
        for (li, ri) in left.iter_mut().zip(right.iter_mut()) {
            for band in &mut self.bands {
                if band.is_idle() {
                    continue;
                }
                band.advance_frame();
                match band.placement {
                    Placement::Stereo => {
                        *li = band.process_channel(*li, 0);
                        *ri = band.process_channel(*ri, 1);
                    }
                    Placement::Left => *li = band.process_channel(*li, 0),
                    Placement::Right => *ri = band.process_channel(*ri, 1),
                    Placement::Mid => {
                        let m = 0.5 * (*li + *ri);
                        let s = 0.5 * (*li - *ri);
                        let m = band.process_channel(m, 0);
                        *li = m + s;
                        *ri = m - s;
                    }
                    Placement::Side => {
                        let m = 0.5 * (*li + *ri);
                        let s = 0.5 * (*li - *ri);
                        let s = band.process_channel(s, 0);
                        *li = m + s;
                        *ri = m - s;
                    }
                }
            }
        }
    }
}

impl Default for EqChain {
    fn default() -> Self {
        Self::new()
    }
}
