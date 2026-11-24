//! Finding hits with spectral flux and placing them with the envelope.
//!
//! The two detectors in this crate each fail at the other's job.
//!
//! [`crate::audio::gate`] — two envelope followers racing — is sample-accurate,
//! which is what quantizing needs, but its thresholds are absolute. Set
//! them for a verse and the choruses smear; set them for a chorus and
//! the verse goes missing. Calibrating it against the album's drum MIDI
//! showed the cost: at the shipped settings it found about a fifth of
//! what the drummer played, and no single sensitivity worked across the
//! six projects.
//!
//! [`crate::audio::onsets`] — spectral flux against a moving median — has no
//! such dial. It asks whether *this* frame changed more than its
//! neighbourhood usually does, so it follows the material instead of
//! being told about it. But an STFT answers to the nearest hop: 5.3 ms
//! at 48 kHz with the default 256-sample hop. Quantizing to that
//! replaces one timing error with another the same size, which is why
//! flux was kept out of the quantize path.
//!
//! Neither objection applies to the two together. Flux decides **that**
//! a hit happened, at whatever resolution it likes; the envelope then
//! decides **when**, by hunting the steepest rise in a window around it.
//! The window is deliberately smaller than the gap between two hits, so
//! refinement can only sharpen a hit's position, never move it onto a
//! neighbour.
//!
//! This is the standard shape in the onset-detection literature: a
//! detection function that is robust but coarse, followed by a
//! localisation step. See Böck & Widmer, *Maximum Filter Vibrato
//! Suppression for Onset Detection* (DAFx-13) for the peak-picking side
//! of it — the adaptive-median formulation `onsets` already uses.

use crate::audio::detect::Transient;
use crate::audio::gate::Hit;
use crate::audio::group_detect::refine_onset;
use crate::audio::onsets::{self, OnsetConfig};

/// How the two stages are put together.
#[derive(Clone, Copy, Debug)]
pub struct HybridConfig {
    /// The flux stage — what decides a hit happened.
    pub onsets: OnsetConfig,
    /// How far either side of the flux frame to hunt for the attack.
    ///
    /// About twice the hop, so the true onset is inside it, and far
    /// less than the gap between two hits, so it cannot reach the
    /// neighbour. Widening this does not find more hits — flux has
    /// already decided how many there are — it only risks moving one
    /// onto the wrong attack.
    pub refine_window_secs: f64,
    /// Shortest gap between two hits after refinement.
    ///
    /// Applied again here because refinement moves hits, and two that
    /// flux placed a hop apart can land on the same attack.
    pub min_spacing_secs: f64,
}

impl Default for HybridConfig {
    fn default() -> Self {
        Self {
            onsets: OnsetConfig {
                // Half the module's own default hop. The refinement
                // stage decides a hit's position either way, but a
                // finer hop separates hits that fall in one frame, and
                // the sweep prefers it at every threshold tried.
                hop: 128,
                ..OnsetConfig::default()
            },
            refine_window_secs: 0.012,
            min_spacing_secs: 0.030,
        }
    }
}

/// Peak and RMS over a short window from `start`, for ranking a hit.
fn measure(samples: &[f64], start: usize, len: usize) -> (f64, f64) {
    let end = (start + len).min(samples.len());
    if end <= start {
        return (0.0, 0.0);
    }
    let slice = &samples[start..end];
    let peak = slice.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let rms = (slice.iter().map(|v| v * v).sum::<f64>() / slice.len() as f64).sqrt();
    (peak, rms)
}

/// Detect hits: spectral flux to find them, the envelope to place them.
// r[impl drums.detect.hybrid]
pub fn detect(samples: &[f64], sample_rate: f64, cfg: &HybridConfig) -> Vec<Transient> {
    if samples.is_empty() || sample_rate <= 0.0 {
        return Vec::new();
    }
    // A 10 ms window is past the attack of any struck sound and short
    // enough that a fast pattern never measures its neighbour.
    let measure_len = (sample_rate * 0.010).max(1.0) as usize;

    let mut out: Vec<Transient> = Vec::new();
    for onset in onsets::detect(samples, sample_rate, cfg.onsets) {
        let coarse = onsets::onset_seconds(&onset, sample_rate, &cfg.onsets);
        let at = refine_onset(samples, sample_rate, coarse, cfg.refine_window_secs);
        let sample = (at * sample_rate).max(0.0) as usize;
        let (peak, rms) = measure(samples, sample, measure_len);
        // Crest is how struck the sound is: a transient has a peak well
        // above its own energy, a swell does not.
        let crest_db = if rms > 1e-12 {
            20.0 * (peak / rms).log10()
        } else {
            0.0
        };
        out.push(Transient {
            at,
            loudness: peak,
            crest_db,
            hit: Hit {
                sample,
                peak,
                rms,
                crest_db,
            },
        });
    }

    out.sort_by(|a, b| a.at.total_cmp(&b.at));
    // Refinement can pull two flux frames onto one attack; keep the
    // louder, which is the one that actually carries the hit.
    let mut deduped: Vec<Transient> = Vec::with_capacity(out.len());
    for t in out {
        match deduped.last_mut() {
            Some(last) if t.at - last.at < cfg.min_spacing_secs => {
                if t.loudness > last.loudness {
                    *last = t;
                }
            }
            _ => deduped.push(t),
        }
    }

    // Loudness is a rank, not a level: normalised to the take so the
    // panel's sensitivity means the same thing on a quiet song.
    let loudest = deduped.iter().fold(0.0f64, |m, t| m.max(t.loudness));
    if loudest > 0.0 {
        for t in &mut deduped {
            t.loudness /= loudest;
        }
    }
    deduped
}
