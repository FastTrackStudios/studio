//! EQ chain managing up to 24 bands using the pro design pipeline.
//!
//! Equivalent to Pro-Q 4's per-channel processing chain that cascades
//! all enabled bands in series.

use crate::eq::runtime::band::Band;

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
    pub fn add_band(&mut self) -> Result<usize, crate::eq::Error> {
        if self.bands.len() >= self.capacity {
            return Err(crate::eq::Error::CapacityExceeded);
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
        use crate::eq::runtime::band::Placement;
        // Idle chain: zero per-sample work.
        if !self.has_active_bands() {
            return;
        }
        // Band by band over the block, not sample by sample through every
        // band: a band's output at a sample depends only on its own state
        // and the band before it at that sample, so the order is free, and
        // one band's loop keeps its coefficients in registers and runs its
        // channels side by side. Bit for bit what the sample-outer loop
        // gave. A band idle once stays idle (disabled, its ramp at zero).
        for band in &mut self.bands {
            for (li, ri) in left.iter_mut().zip(right.iter_mut()) {
                if band.is_idle() {
                    break;
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

#[cfg(test)]
mod tests {
    use super::EqChain;
    use crate::eq::design::FilterType;
    use crate::eq::runtime::band::Placement;

    /// The chain as it ran: sample by sample, through every band.
    fn sample_outer(chain: &mut EqChain, left: &mut [f64], right: &mut [f64]) {
        for (li, ri) in left.iter_mut().zip(right.iter_mut()) {
            for band in &mut chain.bands {
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

    fn chain() -> EqChain {
        let mut c = EqChain::new();
        let bands = [
            (FilterType::Highpass, 80.0, 0.0, 4, Placement::Stereo),
            (FilterType::Peak, 400.0, -4.0, 2, Placement::Mid),
            (FilterType::HighShelf, 6000.0, 3.0, 2, Placement::Side),
            (FilterType::Peak, 2500.0, 5.0, 2, Placement::Left),
            (FilterType::Bandpass, 120.0, 0.0, 2, Placement::Right),
        ];
        for (ty, hz, db, order, placement) in bands {
            let i = c.add_band().unwrap_or(0);
            if let Some(b) = c.band_mut(i) {
                b.filter_type = ty;
                b.freq_hz = hz;
                b.gain_db = db;
                b.order = order;
                b.placement = placement;
            }
            c.update_band(i);
        }
        c
    }

    /// Band by band over the block is sample by sample, bit for bit —
    /// through a band switching off (its ramp to idle) and back on, and a
    /// redesign's crossfade.
    #[test]
    fn band_outer_is_the_sample_outer_chain() {
        let (mut ours, mut theirs) = (chain(), chain());
        let mut seed = 0x9e37_79b9_u32;
        for block in 0..40 {
            match block {
                10 => {
                    for c in [&mut ours, &mut theirs] {
                        if let Some(b) = c.band_mut(1) {
                            b.enabled = false;
                        }
                    }
                }
                25 => {
                    for c in [&mut ours, &mut theirs] {
                        if let Some(b) = c.band_mut(1) {
                            b.enabled = true;
                        }
                        if let Some(b) = c.band_mut(3) {
                            b.gain_db = -6.0;
                        }
                        c.update_band(3);
                    }
                }
                _ => {}
            }
            let len = if block % 3 == 0 { 97 } else { 256 };
            let mut l: Vec<f64> = (0..len)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    f64::from(seed) / f64::from(u32::MAX) - 0.5
                })
                .collect();
            let mut r: Vec<f64> = l.iter().rev().map(|x| 0.7 * x).collect();
            let (mut l2, mut r2) = (l.clone(), r.clone());
            ours.process(&mut l, &mut r);
            sample_outer(&mut theirs, &mut l2, &mut r2);
            for (i, (a, b)) in l.iter().chain(&r).zip(l2.iter().chain(&r2)).enumerate() {
                assert_eq!(a.to_bits(), b.to_bits(), "block {block}, sample {i}");
            }
        }
    }
}
