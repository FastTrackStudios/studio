//! Portable trigger-waveform math + SVG path generation.
//!
//! The pure core of the trigger analysis display — ported from the legacy
//! FTS-Trigger `TriggerWaveform` (scrolling input-peak bars, dB scale
//! markers, draggable threshold line, trigger-event markers), remapped from
//! the legacy linear-amplitude y axis onto the dB-linear axis the comp graph
//! uses (0 dBFS at the top … −[`RANGE_DB`] at the bottom), so a threshold
//! drag lands exactly on the pointer's dB. No plugin framework, no Dioxus —
//! compiles for every target (same split as comp-ui's `comp_graph_svg`). The
//! `native`-gated [`crate::trigger::ui::trigger_waveform`] component renders these paths
//! and adds the interactions.

use std::fmt::Write as _;

/// Display dB range (0 dBFS at the top … −`RANGE_DB` at the bottom).
/// Matches the threshold param's −60..0 dB range, so pointer y ↔ threshold
/// dB is a straight linear map.
pub const RANGE_DB: f32 = 60.0;

/// dB scale grid: the legacy display's marker set.
pub const DB_MARKERS: &[(f32, &str)] = &[
    (0.0, "0"),
    (-6.0, "-6"),
    (-12.0, "-12"),
    (-24.0, "-24"),
    (-48.0, "-48"),
];

/// dB (0 at the top … −[`RANGE_DB`] at the bottom) → y within a graph of
/// height `h`.
#[must_use]
pub fn db_to_y(db: f64, h: f64) -> f64 {
    ((-db) / f64::from(RANGE_DB)).clamp(0.0, 1.0) * h
}

/// Inverse of [`db_to_y`]: y within a graph of height `h` → dB (clamped to
/// the display range, 0 … −[`RANGE_DB`]).
#[must_use]
pub fn y_to_db(y: f64, h: f64) -> f64 {
    -(y / h).clamp(0.0, 1.0) * f64::from(RANGE_DB)
}

/// y of the threshold line for a threshold in dB.
#[must_use]
pub fn threshold_line_y(threshold_db: f32, h: f64) -> f64 {
    db_to_y(f64::from(threshold_db), h)
}

/// Linear peak sample (0..1) → normalized display amplitude (0..1).
///
/// Converted through dB, so the bar heights are log-scaled onto the same
/// axis as the threshold line (a bar exactly reaches the line when its peak
/// equals the threshold).
#[must_use]
pub fn scale_peak(peak: f32) -> f32 {
    if peak <= 0.0 {
        0.0
    } else {
        let db = 20.0 * peak.log10();
        (1.0 + db / RANGE_DB).clamp(0.0, 1.0)
    }
}

/// Map a slice of linear input peaks through [`scale_peak`].
#[must_use]
pub fn scale_peaks(peaks: &[f32]) -> Vec<f32> {
    peaks.iter().map(|&p| scale_peak(p)).collect()
}

/// One SVG path containing every peak-bar column.
///
/// For each normalized sample, a rect subpath from the bottom edge up to its
/// display amplitude. Columns get a 1-px-ish gap (20 % of the column) like
/// the legacy bar rendering; near-silent columns are skipped so an idle
/// display is an empty path.
#[must_use]
pub fn bars_path(samples: &[f32], w: f64, h: f64) -> String {
    let n = samples.len();
    if n == 0 {
        return String::new();
    }
    let step = w / n as f64;
    let bar_w = (step * 0.8).max(0.5);
    let mut d = String::new();
    for (i, &s) in samples.iter().enumerate() {
        let amp = f64::from(s.clamp(0.0, 1.0));
        if amp < 0.001 {
            continue;
        }
        let x = i as f64 * step;
        let y = h - amp * h;
        let _ = write!(
            d,
            "M {x:.1} {h:.1} L {x:.1} {y:.1} L {:.1} {y:.1} L {:.1} {h:.1} Z ",
            x + bar_w,
            x + bar_w,
        );
    }
    d
}

/// Place hit markers over the scrolling window.
///
/// `hits` are `(block_index, velocity)` pairs from the shared hit ring —
/// `block_index` is the monotonic count of processed blocks at the time of
/// the hit, `head` is the wave ring's current monotonic block count, and
/// `len` is the window length in columns. Hits still inside the window come
/// back as `(x_center, velocity)` with the newest block at the right edge
/// (column `len − 1`), exactly aligned with the wave ring's oldest→newest
/// snapshot; hits that scrolled out (or from the future — a torn read) are
/// dropped.
#[must_use]
pub fn marker_columns(hits: &[(u64, f32)], head: u64, len: usize, w: f64) -> Vec<(f64, f32)> {
    if len == 0 {
        return Vec::new();
    }
    let step = w / len as f64;
    let mut out = Vec::new();
    for &(block, vel) in hits {
        if block >= head {
            continue; // future/torn — not in the window yet
        }
        let age = head - block; // 1 = newest pushed block
        if age > len as u64 {
            continue; // scrolled out
        }
        let idx = len as u64 - age; // 0 = oldest column, len-1 = newest
        out.push(((idx as f64 + 0.5) * step, vel.clamp(0.0, 1.0)));
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────
