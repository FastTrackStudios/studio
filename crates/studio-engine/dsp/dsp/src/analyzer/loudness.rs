//! Loudness-weighted comparison (ITU-R BS.1770).
//!
//! The middle metric between a null test and a decay fit. A null says
//! "bit-identical or not", a decay fit says "rings for the same time"; this
//! says "sits at the same loudness, with the same weight in each band" — which
//! is what decides whether a translated preset drops into a mix at the level
//! the engineer expected.

use crate::analyzer::filters::{OCTAVE_CENTRES_HZ, octave_bands};
use crate::analyzer::null::rms;
use dsp_core::audio::loudness::KWeightingFilter;

/// Offset from the BS.1770 mean-square sum to LUFS.
const LUFS_OFFSET: f64 = -0.691;

/// Integrated K-weighted loudness of a mono buffer, in LUFS.
///
/// This is the ungated measurement — appropriate here because the analyzer
/// feeds continuous, deliberately-chosen stimulus rather than programme
/// material, so BS.1770's gating (which exists to ignore silence between
/// dialogue) would only add variance.
#[must_use]
pub fn loudness_lufs(x: &[f32], sample_rate: f64) -> f64 {
    if x.is_empty() {
        return f64::NEG_INFINITY;
    }
    // K-weighting: dsp-core's BS.1770 filter, the one every loudness
    // measurement in the stack uses.
    let mut k = KWeightingFilter::new();
    k.update(sample_rate);
    let mean_square = x
        .iter()
        .map(|&s| {
            let y = k.tick(f64::from(s));
            y * y
        })
        .sum::<f64>()
        / x.len() as f64;
    if mean_square <= 0.0 {
        return f64::NEG_INFINITY;
    }
    LUFS_OFFSET + 10.0 * mean_square.log10()
}

/// Per-octave-band energy of a buffer, in dB.
///
/// Bands follow [`OCTAVE_CENTRES_HZ`]. A silent band reports `-inf`.
#[must_use]
pub fn band_levels_db(x: &[f32], sample_rate: f64) -> Vec<(f64, f64)> {
    octave_bands(x, sample_rate)
        .into_iter()
        .enumerate()
        .map(|(i, band)| {
            let r = rms(&band);
            let db = if r <= 0.0 {
                f64::NEG_INFINITY
            } else {
                20.0 * r.log10()
            };
            (OCTAVE_CENTRES_HZ[i], db)
        })
        .collect()
}

/// One octave band of a loudness comparison.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandDifference {
    pub centre_hz: f64,
    pub reference_db: f64,
    pub candidate_db: f64,
    /// Candidate − reference, in dB. `None` when either side is silent, since
    /// a difference against `-inf` is not a number worth reporting.
    pub difference_db: Option<f64>,
}

/// A loudness-weighted comparison of two renders.
#[derive(Debug, Clone, PartialEq)]
pub struct LoudnessComparison {
    pub reference_lufs: f64,
    pub candidate_lufs: f64,
    /// Candidate − reference, in LU.
    pub loudness_difference_lu: f64,
    pub bands: Vec<BandDifference>,
    /// Largest absolute band difference, over bands where both sides had
    /// signal. `None` if no band could be compared.
    pub worst_band_difference_db: Option<f64>,
}

impl LoudnessComparison {
    /// Whether overall loudness and every comparable band sit within
    /// `tolerance_db`.
    #[must_use]
    pub fn passes(&self, tolerance_db: f64) -> bool {
        if !self.loudness_difference_lu.is_finite()
            || self.loudness_difference_lu.abs() > tolerance_db
        {
            return false;
        }
        match self.worst_band_difference_db {
            Some(w) => w <= tolerance_db,
            // Nothing comparable means nothing verified — do not call that a pass.
            None => false,
        }
    }
}

/// Compare two renders by loudness and per-band balance.
pub fn compare_loudness(
    reference: &[f32],
    candidate: &[f32],
    sample_rate: f64,
) -> LoudnessComparison {
    let reference_lufs = loudness_lufs(reference, sample_rate);
    let candidate_lufs = loudness_lufs(candidate, sample_rate);

    let bands: Vec<BandDifference> = band_levels_db(reference, sample_rate)
        .into_iter()
        .zip(band_levels_db(candidate, sample_rate))
        .map(|((centre_hz, r), (_, c))| BandDifference {
            centre_hz,
            reference_db: r,
            candidate_db: c,
            difference_db: (r.is_finite() && c.is_finite()).then_some(c - r),
        })
        .collect();

    let worst_band_difference_db = bands
        .iter()
        .filter_map(|b| b.difference_db)
        .map(f64::abs)
        .fold(None, |acc: Option<f64>, d| {
            Some(acc.map_or(d, |a| a.max(d)))
        });

    LoudnessComparison {
        reference_lufs,
        candidate_lufs,
        loudness_difference_lu: candidate_lufs - reference_lufs,
        bands,
        worst_band_difference_db,
    }
}
