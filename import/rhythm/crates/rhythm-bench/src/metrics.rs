//! Ground-truth trigger onset extraction and standard 1-to-1 onset evaluation metrics.

use std::collections::HashMap;

use rhythm_hits::{Health, Scan, calibrate, health, seconds, spikes};
use serde::{Deserialize, Serialize};

use crate::rpp::RenderedKit;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PieceGroundTruth {
    pub piece: String,
    pub canonical_class: String,
    pub healthy: bool,
    pub health_note: String,
    pub onsets_sec: Vec<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SongGroundTruth {
    pub song: String,
    pub bpm: f64,
    pub duration_secs: f64,
    pub pieces: Vec<PieceGroundTruth>,
}

/// Normalize detector piece names (close-mic names OR 5/7-class ML labels) into
/// the 3 hardware-triggered ground-truth classes (`kick`, `snare`, `toms`) or
/// cymbal classes (`hihat`, `cymbals`).
#[must_use]
pub fn canonical_class(piece: &str) -> String {
    let p = piece.trim().to_ascii_lowercase();
    match p.as_str() {
        "kick" | "bd" | "kick_in" | "kick_out" | "bass_drum" => "kick".to_owned(),
        "snare" | "sd" | "snare_top" | "snare_bottom" => "snare".to_owned(),
        "t1" | "t2" | "t3" | "t4" | "tom" | "toms" | "tt" | "rack_tom" | "floor_tom" => {
            "toms".to_owned()
        }
        "hh" | "hihat" | "hi_hat" | "hi-hat" | "hh_closed" | "hh_open" => "hihat".to_owned(),
        "cy" | "cym" | "cymbals" | "crash" | "ride" | "ohs" => "cymbals".to_owned(),
        other => other.to_owned(),
    }
}

/// Extract healthy hardware-trigger ground-truth onsets for the 6 triggered kit pieces.
pub fn extract_ground_truth(kit: &RenderedKit) -> SongGroundTruth {
    let spec = [
        ("kick", "kick_in", "kick_trig", "kick"),
        ("snare", "snare_top", "snare_trig", "snare"),
        ("t1", "t1", "t1_trig", "toms"),
        ("t2", "t2", "t2_trig", "toms"),
        ("t3", "t3", "t3_trig", "toms"),
        ("t4", "t4", "t4_trig", "toms"),
    ];

    let mut pieces = Vec::new();
    for (piece_name, mic_key, trig_key, canon) in spec {
        let (Some(mic), Some(trig)) = (kit.tracks.get(mic_key), kit.tracks.get(trig_key)) else {
            continue;
        };
        let raw_spikes = spikes(trig, kit.sample_rate, 28.0, 0.030);
        if raw_spikes.is_empty() {
            pieces.push(PieceGroundTruth {
                piece: piece_name.to_owned(),
                canonical_class: canon.to_owned(),
                healthy: false,
                health_note: "no trigger spikes".to_owned(),
                onsets_sec: Vec::new(),
            });
            continue;
        }
        let scan = Scan::of(mic, kit.sample_rate);
        let cal = calibrate(&scan, &raw_spikes, 0.030);
        let h = health(cal.as_ref());
        let (healthy, note) = match h {
            Health::Working => (true, "working".to_owned()),
            Health::MicSilent(msg) => (true, format!("mic_silent: {msg}")),
            Health::Dead(msg) => (false, format!("dead: {msg}")),
        };
        let onsets_sec = raw_spikes
            .iter()
            .map(|&s| seconds(s, kit.sample_rate))
            .collect();
        pieces.push(PieceGroundTruth {
            piece: piece_name.to_owned(),
            canonical_class: canon.to_owned(),
            healthy,
            health_note: note,
            onsets_sec,
        });
    }

    SongGroundTruth {
        song: kit.song_name.clone(),
        bpm: kit.bpm,
        duration_secs: kit.duration_secs,
        pieces,
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClassMetrics {
    pub truth_count: usize,
    pub pred_count: usize,
    pub tp_20ms: usize,
    pub fp_20ms: usize,
    pub fn_20ms: usize,
    pub precision_20ms: f64,
    pub recall_20ms: f64,
    pub f1_20ms: f64,
    pub tp_50ms: usize,
    pub f1_50ms: f64,
    pub median_abs_err_ms: f64,
    pub p95_abs_err_ms: f64,
    pub median_signed_err_ms: f64,
}

fn greedy_match(truth: &[f64], pred: &[f64], tol_secs: f64) -> Vec<(usize, usize, f64)> {
    let mut pairs: Vec<(usize, usize, f64)> = Vec::new();
    for (ti, &t) in truth.iter().enumerate() {
        for (pi, &p) in pred.iter().enumerate() {
            let err = p - t;
            if err.abs() <= tol_secs {
                pairs.push((ti, pi, err));
            }
        }
    }
    pairs.sort_by(|a, b| a.2.abs().total_cmp(&b.2.abs()));
    let mut used_t = vec![false; truth.len()];
    let mut used_p = vec![false; pred.len()];
    let mut matched = Vec::new();
    for (ti, pi, err) in pairs {
        if !used_t[ti] && !used_p[pi] {
            used_t[ti] = true;
            used_p[pi] = true;
            matched.push((ti, pi, err));
        }
    }
    matched
}

fn prf(tp: usize, pred: usize, truth: usize) -> (f64, f64, f64) {
    let p = if pred == 0 {
        if truth == 0 { 1.0 } else { 0.0 }
    } else {
        (tp as f64) / (pred as f64)
    };
    let r = if truth == 0 {
        if pred == 0 { 1.0 } else { 0.0 }
    } else {
        (tp as f64) / (truth as f64)
    };
    let f1 = if p + r == 0.0 {
        0.0
    } else {
        2.0 * p * r / (p + r)
    };
    (p, r, f1)
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Score predicted onsets against ground truth for a single class.
pub fn score_class(truth: &[f64], pred: &[f64]) -> ClassMetrics {
    let m20 = greedy_match(truth, pred, 0.020);
    let m50 = greedy_match(truth, pred, 0.050);

    let tp_20 = m20.len();
    let fp_20 = pred.len().saturating_sub(tp_20);
    let fn_20 = truth.len().saturating_sub(tp_20);
    let (p20, r20, f1_20) = prf(tp_20, pred.len(), truth.len());

    let tp_50 = m50.len();
    let (_p50, _r50, f1_50) = prf(tp_50, pred.len(), truth.len());

    let mut abs_errs: Vec<f64> = m50.iter().map(|&(_, _, e)| e.abs() * 1000.0).collect();
    abs_errs.sort_by(f64::total_cmp);
    let mut signed_errs: Vec<f64> = m50.iter().map(|&(_, _, e)| e * 1000.0).collect();
    signed_errs.sort_by(f64::total_cmp);

    ClassMetrics {
        truth_count: truth.len(),
        pred_count: pred.len(),
        tp_20ms: tp_20,
        fp_20ms: fp_20,
        fn_20ms: fn_20,
        precision_20ms: p20,
        recall_20ms: r20,
        f1_20ms: f1_20,
        tp_50ms: tp_50,
        f1_50ms: f1_50,
        median_abs_err_ms: percentile(&abs_errs, 0.50),
        p95_abs_err_ms: percentile(&abs_errs, 0.95),
        median_signed_err_ms: percentile(&signed_errs, 0.50),
    }
}

/// Merge healthy ground-truth onsets into canonical classes (`kick`, `snare`, `toms`).
/// Note: if a song has any dead tom trigger, we only include the healthy toms
/// in both truth and (for per-tom detectors) predictions.
pub fn grouped_truth_and_preds(
    gt: &SongGroundTruth,
    preds_by_piece: &[(String, f64)],
) -> HashMap<String, (Vec<f64>, Vec<f64>)> {
    let mut healthy_pieces: HashMap<String, bool> = HashMap::new();
    let mut truth_by_class: HashMap<String, Vec<f64>> = HashMap::new();
    let mut healthy_classes: HashMap<String, bool> = HashMap::new();

    for p in &gt.pieces {
        healthy_pieces.insert(p.piece.clone(), p.healthy);
        if p.healthy {
            healthy_classes.insert(p.canonical_class.clone(), true);
            truth_by_class
                .entry(p.canonical_class.clone())
                .or_default()
                .extend_from_slice(&p.onsets_sec);
        }
    }

    let mut pred_by_class: HashMap<String, Vec<f64>> = HashMap::new();
    for (piece, at) in preds_by_piece {
        let lower = piece.trim().to_ascii_lowercase();
        // If this is a specific close-mic piece (e.g. t4) whose trigger was dead, skip it.
        if let Some(&is_healthy) = healthy_pieces.get(&lower)
            && !is_healthy
        {
            continue;
        }
        let cls = canonical_class(&lower);
        if healthy_classes.get(&cls).copied().unwrap_or(false) {
            pred_by_class.entry(cls).or_default().push(*at);
        }
    }

    let mut out = HashMap::new();
    for (cls, mut truth) in truth_by_class {
        truth.sort_by(f64::total_cmp);
        // Deduplicate near-simultaneous multi-tom trigger hits within 10 ms.
        let mut dedup_t: Vec<f64> = Vec::with_capacity(truth.len());
        for t in truth {
            if dedup_t.last().is_none_or(|&prev| (t - prev) >= 0.010) {
                dedup_t.push(t);
            }
        }
        let mut pred = pred_by_class.remove(&cls).unwrap_or_default();
        pred.sort_by(f64::total_cmp);
        out.insert(cls, (dedup_t, pred));
    }
    out
}

pub fn aggregate_metrics(per_song: &[ClassMetrics], all_abs_errs: &[f64]) -> ClassMetrics {
    let truth_count: usize = per_song.iter().map(|m| m.truth_count).sum();
    let pred_count: usize = per_song.iter().map(|m| m.pred_count).sum();
    let tp_20: usize = per_song.iter().map(|m| m.tp_20ms).sum();
    let tp_50: usize = per_song.iter().map(|m| m.tp_50ms).sum();
    let (p20, r20, f1_20) = prf(tp_20, pred_count, truth_count);
    let (_p50, _r50, f1_50) = prf(tp_50, pred_count, truth_count);
    let mut errs = all_abs_errs.to_vec();
    errs.sort_by(f64::total_cmp);
    ClassMetrics {
        truth_count,
        pred_count,
        tp_20ms: tp_20,
        fp_20ms: pred_count.saturating_sub(tp_20),
        fn_20ms: truth_count.saturating_sub(tp_20),
        precision_20ms: p20,
        recall_20ms: r20,
        f1_20ms: f1_20,
        tp_50ms: tp_50,
        f1_50ms: f1_50,
        median_abs_err_ms: percentile(&errs, 0.50),
        p95_abs_err_ms: percentile(&errs, 0.95),
        median_signed_err_ms: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_piece_normalization() {
        assert_eq!(canonical_class("BD"), "kick");
        assert_eq!(canonical_class("Kick_In"), "kick");
        assert_eq!(canonical_class("SD"), "snare");
        assert_eq!(canonical_class("t3"), "toms");
        assert_eq!(canonical_class("TT"), "toms");
        assert_eq!(canonical_class("HH_open"), "hihat");
        assert_eq!(canonical_class("CY"), "cymbals");
    }

    #[test]
    fn one_to_one_onset_matching_20ms_and_50ms() {
        let truth = [1.000, 2.000, 3.000];
        let pred = [1.005, 1.012, 2.035, 5.000];
        let m = score_class(&truth, &pred);
        // Within 20 ms: only 1.005 matches 1.000 (1.012 is extra FP, 2.035 is >20ms)
        assert_eq!(m.tp_20ms, 1);
        assert_eq!(m.fp_20ms, 3);
        assert_eq!(m.fn_20ms, 2);
        // Within 50 ms: 1.005 matches 1.000 AND 2.035 matches 2.000
        assert_eq!(m.tp_50ms, 2);
        assert!((m.median_abs_err_ms - 5.0).abs() < 1e-6 || (m.median_abs_err_ms - 35.0).abs() < 1e-6);
    }
}
