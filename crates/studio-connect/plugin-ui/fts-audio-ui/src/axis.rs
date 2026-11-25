//! Frequency / dB axis mapping primitives.
//!
//! These are pure math helpers shared by spectrum analyzers, EQ graphs,
//! response viewers, and any audio viz that needs a log-frequency or
//! symmetric-dB coordinate space. Provider-agnostic — the consumer brings
//! its own pixel rect; this module just handles the value↔position math.

/// Logarithmic frequency axis between two endpoints.
///
/// Maps a frequency in Hz to a normalized `[0.0, 1.0]` position (and back),
/// using `log10` so a decade occupies an equal amount of screen real
/// estate. Suitable for the X axis of every typical audio frequency
/// display.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FreqAxis {
    pub min_hz: f64,
    pub max_hz: f64,
    log_min: f64,
    log_span: f64,
}

impl FreqAxis {
    /// New axis from `min_hz` to `max_hz`. Both must be positive and
    /// `max > min`; debug builds assert this.
    pub fn new(min_hz: f64, max_hz: f64) -> Self {
        debug_assert!(min_hz > 0.0, "FreqAxis min must be positive");
        debug_assert!(max_hz > min_hz, "FreqAxis max must be > min");
        let log_min = min_hz.log10();
        let log_max = max_hz.log10();
        Self {
            min_hz,
            max_hz,
            log_min,
            log_span: log_max - log_min,
        }
    }

    /// Standard 20 Hz – 20 kHz audible range.
    pub fn audible() -> Self {
        Self::new(20.0, 20_000.0)
    }

    /// Convert a frequency to a `[0.0, 1.0]` position. Out-of-range values
    /// map outside that interval — clamp at the call site if you need
    /// strict bounds.
    #[inline]
    pub fn freq_to_norm(&self, freq_hz: f64) -> f64 {
        (freq_hz.log10() - self.log_min) / self.log_span
    }

    /// Convert a `[0.0, 1.0]` normalized position back to a frequency in
    /// Hz. Inverse of [`Self::freq_to_norm`].
    #[inline]
    pub fn norm_to_freq(&self, norm: f64) -> f64 {
        10.0_f64.powf(self.log_min + norm * self.log_span)
    }

    /// Map a frequency to a pixel x-coordinate within `[x0, x1]`.
    #[inline]
    pub fn freq_to_x(&self, freq_hz: f64, x0: f64, x1: f64) -> f64 {
        x0 + self.freq_to_norm(freq_hz) * (x1 - x0)
    }

    /// Inverse of [`Self::freq_to_x`].
    #[inline]
    pub fn x_to_freq(&self, x: f64, x0: f64, x1: f64) -> f64 {
        let norm = (x - x0) / (x1 - x0);
        self.norm_to_freq(norm)
    }

    /// Generate `count` log-spaced sample frequencies covering the axis,
    /// inclusive at both ends. Useful for sampling a frequency response
    /// curve at evenly-spaced screen pixels.
    pub fn log_spaced(&self, count: usize) -> Vec<f64> {
        if count <= 1 {
            return vec![self.min_hz];
        }
        let denom = (count - 1) as f64;
        (0..count)
            .map(|i| self.norm_to_freq(i as f64 / denom))
            .collect()
    }
}

/// Symmetric dB axis ranged at `±range_db` from 0 dB. Linear mapping —
/// 0 dB is the centre of the screen; positive values toward the top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DbAxis {
    pub range_db: f64,
}

impl DbAxis {
    /// New axis with `±range_db` half-span. Common values: 6, 12, 18, 24, 30.
    pub fn symmetric(range_db: f64) -> Self {
        debug_assert!(range_db > 0.0, "DbAxis range must be positive");
        Self { range_db }
    }

    /// Map a dB value to a `[0.0, 1.0]` position. **Top-origin** (0 dB
    /// at norm = 0.5; positive dB → smaller norm). Matches typical
    /// graph-viewer conventions where Y increases downward.
    #[inline]
    pub fn db_to_norm(&self, db: f64) -> f64 {
        let clamped = db.clamp(-self.range_db, self.range_db);
        0.5 - clamped / (2.0 * self.range_db)
    }

    /// Inverse of [`Self::db_to_norm`].
    #[inline]
    pub fn norm_to_db(&self, norm: f64) -> f64 {
        (0.5 - norm) * 2.0 * self.range_db
    }

    /// Map a dB value to a pixel y-coordinate within `[y0, y1]`.
    #[inline]
    pub fn db_to_y(&self, db: f64, y0: f64, y1: f64) -> f64 {
        y0 + self.db_to_norm(db) * (y1 - y0)
    }

    /// Inverse of [`Self::db_to_y`].
    #[inline]
    pub fn y_to_db(&self, y: f64, y0: f64, y1: f64) -> f64 {
        let norm = (y - y0) / (y1 - y0);
        self.norm_to_db(norm)
    }
}
