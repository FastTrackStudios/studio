//! The comparison report: run all three metrics over a reference/candidate
//! pair and say whether the candidate matches.
//!
//! The three metrics answer different questions and are deliberately not
//! collapsed into one score:
//!
//! - [`crate::null`] — *is it the same processing?* Strict. The right bar for
//!   FTS-EQ against Pro-Q 4, which share a design pipeline and should null
//!   deeply. Meaningless for reverb.
//! - [`crate::decay`] — *does it ring like the same space?* The reverb bar.
//! - [`crate::loudness`] — *does it sit at the same level and balance?* The
//!   bar that decides whether a translated preset drops into a mix correctly.
//!
//! A profile picks which of the three must pass, because "matching" means
//! something different for an EQ than for a reverb.

use crate::decay::{DecayComparison, DecayFit, compare_decay};
use crate::loudness::{LoudnessComparison, compare_loudness};
use crate::null::{NullTest, null_test};

/// What a given comparison is allowed to differ by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    /// Minimum null depth in dB. `None` disables the null criterion — correct
    /// for reverb, where two different algorithms never null.
    pub min_null_depth_db: Option<f64>,
    /// Maximum per-band RT60 ratio error, where 0.1 means "within 10%".
    /// `None` disables the decay criterion.
    pub max_decay_ratio_error: Option<f64>,
    /// Maximum loudness and per-band difference in dB. `None` disables it.
    pub max_loudness_difference_db: Option<f64>,
    /// Which decay fit to use.
    pub decay_fit: DecayFit,
}

impl Thresholds {
    /// For a processor meant to be numerically identical to its reference —
    /// FTS-EQ against Pro-Q 4. Demands a deep null; decay is irrelevant.
    ///
    /// 60 dB is well below anything audible while still leaving room for the
    /// f32/f64 rounding differences between two implementations.
    #[must_use]
    pub fn exact_match() -> Self {
        Self {
            min_null_depth_db: Some(60.0),
            max_decay_ratio_error: None,
            max_loudness_difference_db: Some(0.1),
            decay_fit: DecayFit::T20,
        }
    }

    /// For a reverb matched to a reference reverb. No null requirement —
    /// judged on decay character and spectral balance.
    #[must_use]
    pub fn reverb_match() -> Self {
        Self {
            min_null_depth_db: None,
            max_decay_ratio_error: Some(0.15),
            max_loudness_difference_db: Some(1.5),
            decay_fit: DecayFit::T20,
        }
    }
}

/// Which criteria a comparison was judged on, and how each fared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Criterion {
    Null,
    Decay,
    Loudness,
}

/// One criterion's outcome.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CriterionResult {
    pub criterion: Criterion,
    pub passed: bool,
    /// The measured value that was thresholded (null depth dB / ratio error /
    /// worst band difference dB).
    pub measured: Option<f64>,
    pub threshold: f64,
}

/// A full comparison of a candidate render against a reference render.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub null: NullTest,
    pub decay: DecayComparison,
    pub loudness: LoudnessComparison,
    /// One entry per *enabled* criterion.
    pub results: Vec<CriterionResult>,
}

impl Comparison {
    /// Whether every enabled criterion passed.
    ///
    /// A comparison with no enabled criteria is **not** a pass — that would
    /// mean reporting success without measuring anything.
    #[must_use]
    pub fn passed(&self) -> bool {
        !self.results.is_empty() && self.results.iter().all(|r| r.passed)
    }

    /// The criteria that failed.
    pub fn failures(&self) -> impl Iterator<Item = &CriterionResult> {
        self.results.iter().filter(|r| !r.passed)
    }
}

/// Compare a candidate render against a reference render.
///
/// Both buffers must already be sample-aligned — use
/// [`crate::null::align_by_latency`] with the plugin's reported latency
/// first, or the null test measures the offset rather than the processing.
///
/// `reference_ir` / `candidate_ir` are the impulse responses used for the
/// decay criterion. Pass the same buffers as the main renders when the
/// stimulus *was* an impulse.
#[must_use]
pub fn compare(
    reference: &[f32],
    candidate: &[f32],
    reference_ir: &[f32],
    candidate_ir: &[f32],
    sample_rate: f64,
    thresholds: Thresholds,
) -> Comparison {
    let null = null_test(reference, candidate);
    let decay = compare_decay(
        reference_ir,
        candidate_ir,
        sample_rate,
        thresholds.decay_fit,
    );
    let loudness = compare_loudness(reference, candidate, sample_rate);

    let mut results = Vec::new();

    if let Some(t) = thresholds.min_null_depth_db {
        results.push(CriterionResult {
            criterion: Criterion::Null,
            passed: null.passes(t),
            measured: null.null_depth_db.is_finite().then_some(null.null_depth_db),
            threshold: t,
        });
    }

    if let Some(t) = thresholds.max_decay_ratio_error {
        // An unmeasurable decay is a failure, not a pass: nothing was verified.
        let measured = decay.worst_ratio_error;
        results.push(CriterionResult {
            criterion: Criterion::Decay,
            passed: measured.is_some_and(|e| e <= t),
            measured,
            threshold: t,
        });
    }

    if let Some(t) = thresholds.max_loudness_difference_db {
        results.push(CriterionResult {
            criterion: Criterion::Loudness,
            passed: loudness.passes(t),
            measured: loudness.worst_band_difference_db,
            threshold: t,
        });
    }

    Comparison {
        null,
        decay,
        loudness,
        results,
    }
}
