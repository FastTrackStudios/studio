//! Portable compressor-graph math + SVG path generation.
//!
//! The pure core of the comp graph — ported from the signal domain's
//! `CompSurface` (`features/rigs/guitar/ui/src/comp_surface.rs`, itself a 1:1
//! port of the audio-gui vello painter).
//!
//! Supports the soft-knee `compress_transfer` curve, the Catmull-Rom
//! `smooth_path` waveform fills, and the dB ↔ y coordinate mapping. No
//! plugin framework, no Dioxus — compiles for every target (same split as
//! eq-ui's `eq_graph_svg`). The `native`-gated [`crate::ui::comp_graph`]
//! component renders these paths and adds the interactions.

use std::fmt::Write;

/// Display dB range (0 dBFS at the top … −`RANGE_DB` at the bottom). 60 dB:
/// matches the reference surface — the −20/−30 dB neighborhood where
/// thresholds live sits mid-window.
pub const RANGE_DB: f32 = 60.0;

/// Soft-knee transfer function — same math as audio-gui's
/// `compress_transfer` / `CompSurface`.
///
/// Below `threshold − knee/2` the output equals the input; above
/// `threshold + knee/2` the output follows the ratio slope; inside the knee
/// a quadratic blends the two with continuous value and first derivative.
#[must_use]
pub fn compress_transfer(input_db: f32, threshold_db: f32, ratio: f32, knee_db: f32) -> f32 {
    if ratio <= 1.0 {
        return input_db;
    }
    let slope = 1.0 - 1.0 / ratio;
    let half_knee = knee_db * 0.5;
    if knee_db > 0.001 && (input_db - threshold_db).abs() < half_knee {
        let x = input_db - threshold_db + half_knee;
        input_db - slope * x * x / (2.0 * knee_db)
    } else if input_db > threshold_db {
        input_db - slope * (input_db - threshold_db)
    } else {
        input_db
    }
}

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

/// Input dB (−[`RANGE_DB`] at the left … 0 at the right) → x within a graph
/// of width `w`.
#[must_use]
pub fn db_to_x(db: f64, w: f64) -> f64 {
    ((db + f64::from(RANGE_DB)) / f64::from(RANGE_DB)).clamp(0.0, 1.0) * w
}

/// Linear peak sample (0..1) → normalized display amplitude (0..1) through
/// dB, so the waveform is log-scaled like the painter's.
#[must_use]
pub fn scale_input_peak(peak: f32) -> f32 {
    if peak <= 0.0 {
        0.0
    } else {
        let db = 20.0 * peak.log10();
        (1.0 + db / RANGE_DB).clamp(0.0, 1.0)
    }
}

/// Gain reduction in dB (positive = reducing) → normalized display fraction
/// of the graph height (0..1), painted downward from the top.
#[must_use]
pub fn scale_gr_db(gr_db: f32) -> f32 {
    (gr_db / RANGE_DB).clamp(0.0, 1.0)
}

/// The colours the compressor's display is drawn in.
///
/// The editor states these inline as SVG gradient stops, which is the
/// right shape for a DOM and the wrong one for anything else: a host
/// that embeds this display — a mixer strip's rack, say — has to paint
/// with the same values or the two are different compressors.
///
/// `#rrggbb`, because that is what the editor writes and what
/// `eq_graph_model::freq_to_color` already returns; a caller that wants
/// them as numbers parses once.
pub mod colors {
    /// The readout grey — labels, units, anything that is not a value.
    pub const GREY: &str = "#8a8a92";
    /// The input waveform's edge, and the fill beneath it.
    pub const INPUT_EDGE: &str = "#3cd2dc";
    pub const INPUT_FILL: &str = "#146e73";
    /// Gain reduction, hanging from the top.
    pub const REDUCTION_EDGE: &str = "#ff5050";
    pub const REDUCTION_FILL: &str = "#dc2828";
    /// The threshold line and its grab chip.
    pub const THRESHOLD: &str = "#ff7878";
}

/// Map a slice of linear input peaks through [`scale_input_peak`].
#[must_use]
pub fn scale_input_wave(peaks: &[f32]) -> Vec<f32> {
    peaks.iter().map(|&p| scale_input_peak(p)).collect()
}

/// Map a slice of GR dB values through [`scale_gr_db`].
#[must_use]
pub fn scale_gr_wave(gr_db: &[f32]) -> Vec<f32> {
    gr_db.iter().map(|&g| scale_gr_db(g)).collect()
}

/// Smooth SVG path through normalized samples (Catmull-Rom → cubic Bézier,
/// the painter's `build_smooth_path`), optionally closed to `baseline` for a
/// fill.
///
/// `from_bottom = true` draws amplitude up from the bottom edge (the
/// input waveform); `false` draws downward from the top (the GR trace).
#[must_use]
pub fn smooth_path(samples: &[f32], w: f64, h: f64, from_bottom: bool, close: bool) -> String {
    let n = samples.len();
    if n == 0 {
        return String::new();
    }
    let step = w / n as f64;
    let ys: Vec<f64> = samples
        .iter()
        .map(|&s| {
            let amp = f64::from(s.clamp(0.0, 1.0));
            if from_bottom { h - amp * h } else { amp * h }
        })
        .collect();
    let baseline = if from_bottom { h } else { 0.0 };
    let mut d = String::new();
    if close {
        let _ = write!(d, "M 0 {baseline:.1} L 0 {:.1} ", ys[0]);
    } else {
        let _ = write!(d, "M 0 {:.1} ", ys[0]);
    }
    for i in 0..n - 1 {
        let x0 = i as f64 * step;
        let x1 = (i + 1) as f64 * step;
        let y_prev = if i > 0 { ys[i - 1] } else { ys[0] };
        let (y_curr, y_next) = (ys[i], ys[i + 1]);
        let y_next2 = if i + 2 < n { ys[i + 2] } else { ys[n - 1] };
        let t1 = (y_next - y_prev) / 2.0;
        let t2 = (y_next2 - y_curr) / 2.0;
        let _ = write!(
            d,
            "C {:.1} {:.1} {:.1} {:.1} {:.1} {:.1} ",
            x0 + step / 3.0,
            y_curr + t1 / 3.0,
            x1 - step / 3.0,
            y_next - t2 / 3.0,
            x1,
            y_next,
        );
    }
    if close {
        let _ = write!(d, "L {w:.1} {baseline:.1} Z");
    }
    d
}

/// SVG polyline path of the transfer curve across the full display range
/// (input −[`RANGE_DB`]..0 dB left→right), 61 sample points.
#[must_use]
pub fn transfer_curve_path(threshold_db: f32, ratio: f32, knee_db: f32, w: f64, h: f64) -> String {
    let mut d = String::new();
    for i in 0..=60 {
        let input = (f64::from(i) / 60.0).mul_add(f64::from(RANGE_DB), -f64::from(RANGE_DB));
        let output = f64::from(compress_transfer(
            input as f32,
            threshold_db,
            ratio,
            knee_db,
        ));
        let x = db_to_x(input, w);
        let y = db_to_y(output, h);
        d.push_str(if i == 0 { "M " } else { "L " });
        let _ = write!(d, "{x:.1} {y:.1} ");
    }
    d
}

/// Graph position of the live input "ball" riding the transfer curve.
#[must_use]
pub fn transfer_ball(
    in_db: f32,
    threshold_db: f32,
    ratio: f32,
    knee_db: f32,
    w: f64,
    h: f64,
) -> (f64, f64) {
    let level = in_db.clamp(-RANGE_DB, 0.0);
    let out = f64::from(compress_transfer(level, threshold_db, ratio, knee_db));
    (db_to_x(f64::from(level), w), db_to_y(out, h))
}

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────
