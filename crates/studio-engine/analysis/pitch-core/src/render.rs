//! Committed (offline) render: [`crate::model::PitchDoc`] edits →
//! WORLD resynthesis.
//!
//! WORLD analyzes once (`f0[]`, spectral envelope, aperiodicity); the
//! edit render REWRITES the f0 contour from the blob targets and
//! resynthesizes — formants stay untouched by construction, which is
//! why this is the quality tier (the realtime preview shifts with
//! PSOLA instead). WORLD's frame grid (~5 ms) differs from the
//! analysis hop grid; frames map through time.

use crate::detect::midi_to_hz;
use crate::model::PitchDoc;
use world_sys::WorldAnalysis;

/// Render the edited document over the original mono audio.
///
/// Frames whose time falls inside an edited blob take the blob's
/// target contour (+ marker bend); everything else keeps the WORLD
/// analysis f0 (unvoiced consonants and unedited regions pass
/// through untouched).
pub fn render_world(doc: &PitchDoc, audio: &[f64], sample_rate: u32) -> Vec<f64> {
    let analysis = WorldAnalysis::analyze(audio, sample_rate, 5.0);
    let mut f0 = analysis.f0.clone();
    for (i, slot) in f0.iter_mut().enumerate() {
        if *slot <= 0.0 {
            continue; // unvoiced — leave it alone
        }
        let t = analysis.temporal_positions[i];
        let sample = t * sample_rate as f64;
        let frame = (sample / doc.hop.max(1) as f64) as usize;
        for blob in &doc.blobs {
            if frame >= blob.start_frame && frame <= blob.end_frame {
                if let Some(target) = blob.target_midi(frame) {
                    *slot = midi_to_hz(target + doc.bend_at(sample));
                }
                break;
            }
        }
    }
    analysis.synthesize_with_f0(&f0)
}

/// Render with the document's TIME-WARP applied (melonix marker
/// model): output frame k at time T reads the analysis frame at the
/// source position where `source_time + warp(source) = T` — WORLD
/// synthesis accepts frames at any rate, so time-stretch is pure
/// frame-index remapping (pitch and formants unaffected).
pub fn render_world_warped(doc: &PitchDoc, audio: &[f64], sample_rate: u32) -> Vec<f64> {
    let analysis = WorldAnalysis::analyze(audio, sample_rate, 5.0);
    let n_src = analysis.frames();
    if n_src == 0 {
        return Vec::new();
    }
    let sr = sample_rate as f64;
    let period_samples = 5.0 / 1000.0 * sr;

    // Edited f0 on the SOURCE grid first (same as render_world).
    let mut f0_src = analysis.f0.clone();
    for (i, slot) in f0_src.iter_mut().enumerate() {
        if *slot <= 0.0 {
            continue;
        }
        let sample = analysis.temporal_positions[i] * sr;
        let frame = (sample / doc.hop.max(1) as f64) as usize;
        for blob in &doc.blobs {
            if frame >= blob.start_frame && frame <= blob.end_frame {
                if let Some(target) = blob.target_midi(frame) {
                    *slot = midi_to_hz(target + doc.bend_at(sample));
                }
                break;
            }
        }
    }

    // Output timeline length: last source sample plus its warp offset.
    let src_end = (n_src - 1) as f64 * period_samples;
    let out_end = src_end + doc.warp_at(src_end);
    let n_out = ((out_end / period_samples) as usize).max(1);

    // Remap: for each output frame, invert the (monotonic) warp by
    // scanning — marker counts are tiny, a linear walk per frame is
    // fine at editing scale.
    let bins = analysis.bins();
    let mut f0 = Vec::with_capacity(n_out);
    let mut sp = Vec::with_capacity(n_out * bins);
    let mut ap = Vec::with_capacity(n_out * bins);
    for k in 0..n_out {
        let t_out = k as f64 * period_samples;
        // Solve src + warp(src) = t_out by bisection over [0, src_end].
        let (mut lo, mut hi) = (0.0f64, src_end);
        for _ in 0..32 {
            let mid = 0.5 * (lo + hi);
            if mid + doc.warp_at(mid) < t_out {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let src_sample = 0.5 * (lo + hi);
        let src_frame = ((src_sample / period_samples).round() as usize).min(n_src - 1);
        f0.push(f0_src[src_frame]);
        sp.extend_from_slice(&analysis.sp[src_frame * bins..(src_frame + 1) * bins]);
        ap.extend_from_slice(&analysis.ap[src_frame * bins..(src_frame + 1) * bins]);
    }
    let warped = WorldAnalysis {
        f0: f0.clone(),
        temporal_positions: (0..n_out).map(|k| k as f64 * period_samples / sr).collect(),
        sp,
        ap,
        fft_size: analysis.fft_size,
        frame_period_ms: analysis.frame_period_ms,
        sample_rate,
    };
    warped.synthesize_with_f0(&f0)
}
