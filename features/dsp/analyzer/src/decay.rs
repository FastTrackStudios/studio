//! Energy-decay and reverberation-time measurement.
//!
//! Two reverbs built on different algorithms will never null against each
//! other, so "does ours match theirs" has to be answered with the properties
//! that make a space sound like itself: how long it rings, and how that time
//! varies with frequency. This module measures both from an impulse response.
//!
//! Method is the standard one (ISO 3382): integrate the squared IR backwards
//! to get the Schroeder energy-decay curve, then fit a line to a segment of it
//! and extrapolate to −60 dB.

use crate::filters::{OCTAVE_CENTRES_HZ, octave_bands};

/// Which segment of the decay curve the fit uses.
///
/// A full −60 dB fit is rarely usable — the tail disappears into the noise
/// floor first — so the convention is to fit a shallower segment and
/// extrapolate. Both start 5 dB down, to skip the direct sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecayFit {
    /// Fit −5 dB → −25 dB, extrapolate ×3. Tolerates a high noise floor.
    T20,
    /// Fit −5 dB → −35 dB, extrapolate ×2. More accurate when the tail is clean.
    T30,
    /// Fit −5 dB → −15 dB, extrapolate ×6. The last resort for short or noisy
    /// tails that never present 20 dB of straight decay — a large
    /// extrapolation, so only worth using when the others cannot fit at all.
    T10,
}

impl DecayFit {
    /// The dB range fitted, and the factor that extrapolates it to 60 dB.
    fn range(self) -> (f64, f64, f64) {
        match self {
            Self::T20 => (-5.0, -25.0, 3.0),
            Self::T30 => (-5.0, -35.0, 2.0),
            Self::T10 => (-5.0, -15.0, 6.0),
        }
    }
}

/// The Schroeder energy-decay curve, in dB relative to total energy.
///
/// `edc[i]` is the energy remaining from sample `i` onward. Monotonically
/// decreasing by construction, which is what makes the level crossings
/// unambiguous.
#[must_use]
pub fn energy_decay_curve(ir: &[f32]) -> Vec<f64> {
    let mut running = 0.0f64;
    // Backwards cumulative sum of squares.
    let mut tail: Vec<f64> = ir
        .iter()
        .rev()
        .map(|&s| {
            running += (s as f64) * (s as f64);
            running
        })
        .collect();
    tail.reverse();

    let total = tail.first().copied().unwrap_or(0.0);
    if total <= 0.0 {
        return vec![f64::NEG_INFINITY; ir.len()];
    }
    tail.iter()
        .map(|&e| {
            if e <= 0.0 {
                f64::NEG_INFINITY
            } else {
                10.0 * (e / total).log10()
            }
        })
        .collect()
}

/// The first sample index at or below `level_db` on a decay curve.
fn crossing(edc: &[f64], level_db: f64) -> Option<usize> {
    edc.iter().position(|&v| v <= level_db)
}

/// Minimum coefficient of determination for a decay fit to be trusted.
///
/// The Schroeder integral is taken over a *finite* buffer, so its curve always
/// plunges toward `-inf` at the end no matter what the signal was — steady
/// noise included. That means "did it cross −25 dB?" is not on its own
/// evidence of a decay, and an unguarded fit happily returns an RT60 for a
/// constant tone.
///
/// What separates a real decay is that its curve is *straight*: an
/// exponentially decaying tail is linear in dB, while the truncation artifact
/// is the sharply-curved `10·log10(1 − t/N)`. Requiring a near-linear fit
/// rejects the artifact. ISO 3382 makes the same distinction with its
/// non-linearity parameter.
const MIN_FIT_R_SQUARED: f64 = 0.98;

/// Reverberation time in seconds, from an impulse response.
///
/// Returns `None` when the response never decays far enough to fit, or when
/// what it does is not a straight decay — a tail that is too short, too
/// noisy, or simply not a decaying response at all. That is a real outcome,
/// not an error: an honest "not measurable" beats a number extrapolated from
/// a truncation artifact.
#[must_use]
pub fn reverb_time(ir: &[f32], sample_rate: f64, fit: DecayFit) -> Option<f64> {
    let edc = energy_decay_curve(ir);
    let (from_db, to_db, factor) = fit.range();

    let start = crossing(&edc, from_db)?;
    let end = crossing(&edc, to_db)?;
    if end <= start {
        return None;
    }

    // Least-squares slope over the fitted segment, in dB per sample.
    let seg = &edc[start..=end];
    if seg.len() < 3 || seg.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let n = seg.len() as f64;
    let mean_x = (n - 1.0) / 2.0;
    let mean_y = seg.iter().sum::<f64>() / n;
    let (mut num, mut den, mut total_ss) = (0.0, 0.0, 0.0);
    for (i, &y) in seg.iter().enumerate() {
        let dx = i as f64 - mean_x;
        let dy = y - mean_y;
        num += dx * dy;
        den += dx * dx;
        total_ss += dy * dy;
    }
    if den == 0.0 || total_ss == 0.0 {
        return None;
    }
    let slope_db_per_sample = num / den;
    if slope_db_per_sample >= 0.0 {
        return None; // not decaying
    }

    // Reject anything that is not a straight line in dB — see
    // MIN_FIT_R_SQUARED. r² = explained variance / total variance.
    let r_squared = (num * num / den) / total_ss;
    if r_squared < MIN_FIT_R_SQUARED {
        return None;
    }

    // Time to fall the fitted range (both terms negative, so this is
    // positive), scaled up to a full 60 dB.
    let samples = (to_db - from_db) / slope_db_per_sample;
    Some(samples * factor / sample_rate)
}

/// Reverberation time, falling back to a shallower fit when the preferred one
/// cannot be made.
///
/// Short reverbs are the reason this exists: a 0.3 s tail may never present a
/// straight 20 dB of decay, and returning `None` for it means the comparison
/// silently declines to measure a perfectly good reverb. Trying `T20` and then
/// `T10` keeps those in scope, at the cost of a larger extrapolation on the
/// ones that need the fallback.
#[must_use]
pub fn reverb_time_best_effort(ir: &[f32], sample_rate: f64, preferred: DecayFit) -> Option<f64> {
    let mut order = vec![preferred];
    for f in [DecayFit::T20, DecayFit::T10] {
        if !order.contains(&f) {
            order.push(f);
        }
    }
    order
        .into_iter()
        .find_map(|fit| reverb_time(ir, sample_rate, fit))
}

/// Reverb time per octave band.
///
/// Index matches [`OCTAVE_CENTRES_HZ`]; a band that cannot be fitted is
/// `None` rather than a fabricated number.
#[must_use]
pub fn reverb_time_per_band(
    ir: &[f32],
    sample_rate: f64,
    fit: DecayFit,
) -> Vec<(f64, Option<f64>)> {
    octave_bands(ir, sample_rate)
        .into_iter()
        .enumerate()
        .map(|(i, band)| {
            (
                OCTAVE_CENTRES_HZ[i],
                reverb_time_best_effort(&band, sample_rate, fit),
            )
        })
        .collect()
}

/// How closely a candidate's decay matches a reference's, per octave band.
#[derive(Debug, Clone, PartialEq)]
pub struct DecayComparison {
    /// `(centre_hz, reference_rt60, candidate_rt60, ratio)` per band.
    ///
    /// `ratio` is candidate ÷ reference — 1.0 is a perfect match. `None`
    /// where either side could not be fitted.
    pub bands: Vec<DecayBand>,
    /// Largest absolute deviation of `ratio` from 1.0, over the bands that
    /// could be compared. `None` if no band could be.
    pub worst_ratio_error: Option<f64>,
}

/// One octave band of a decay comparison.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecayBand {
    pub centre_hz: f64,
    pub reference_s: Option<f64>,
    pub candidate_s: Option<f64>,
    pub ratio: Option<f64>,
}

/// Compare two impulse responses band by band.
#[must_use]
pub fn compare_decay(
    reference_ir: &[f32],
    candidate_ir: &[f32],
    sample_rate: f64,
    fit: DecayFit,
) -> DecayComparison {
    let r = reverb_time_per_band(reference_ir, sample_rate, fit);
    compare_decay_against(&r, candidate_ir, sample_rate, fit)
}

/// As [`compare_decay`], but against a reference whose bands were measured
/// earlier.
///
/// Band-splitting a buffer is the expensive half of a comparison — eight
/// bands, three biquad sections each, over the whole tail. A tuning loop
/// compares against the same reference dozens of times, so measuring it once
/// and reusing it removes half the work from every iteration.
#[must_use]
pub fn compare_decay_against(
    reference_bands: &[(f64, Option<f64>)],
    candidate_ir: &[f32],
    sample_rate: f64,
    fit: DecayFit,
) -> DecayComparison {
    let r = reference_bands.to_vec();
    let c = reverb_time_per_band(candidate_ir, sample_rate, fit);

    let bands: Vec<DecayBand> = r
        .into_iter()
        .zip(c)
        .map(|((centre_hz, rs), (_, cs))| {
            let ratio = match (rs, cs) {
                (Some(rv), Some(cv)) if rv > 0.0 => Some(cv / rv),
                _ => None,
            };
            DecayBand {
                centre_hz,
                reference_s: rs,
                candidate_s: cs,
                ratio,
            }
        })
        .collect();

    let worst_ratio_error = bands
        .iter()
        .filter_map(|b| b.ratio)
        .map(|r| (r - 1.0).abs())
        .fold(None, |acc: Option<f64>, e| {
            Some(acc.map_or(e, |a| a.max(e)))
        });

    DecayComparison {
        bands,
        worst_ratio_error,
    }
}
