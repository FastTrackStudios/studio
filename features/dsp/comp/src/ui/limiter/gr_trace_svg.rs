//! Geometry for the limiter's scrolling gain-reduction trace.
//!
//! Portable on purpose — no Dioxus, no `nice_plug`, no plugin framework — so the
//! shape math is unit-testable and can be reused by a wasm remote. The
//! component in [`crate::ui::limiter::gr_trace`] does nothing but feed these functions and
//! emit SVG.
//!
//! The view is a time window: oldest sample on the left, newest on the right.
//! Two layers share it — the output waveform as a filled envelope around the
//! centre line, and the gain reduction hanging down from the top.

use std::fmt::Write;

/// Trace height in CSS px. The component pins its container to this so pointer
/// y maps 1:1 onto the viewBox.
pub const TRACE_H: f64 = 180.0;

/// Trace width in viewBox units.
pub const TRACE_W: f64 = 600.0;

/// Gain reduction (dB) shown at the bottom of the trace. Beyond this the trace
/// clamps — a limiter pulling more than 24 dB is off the rails anyway.
pub const GR_RANGE_DB: f64 = 24.0;

/// Map a gain-reduction amount in dB to a y coordinate.
///
/// 0 dB (no reduction) sits at the top; [`GR_RANGE_DB`] at the bottom.
#[must_use]
pub fn gr_to_y(gr_db: f64, height: f64) -> f64 {
    let t = (gr_db.max(0.0) / GR_RANGE_DB).min(1.0);
    t * height
}

/// Map a linear sample magnitude (0..=1) to a half-height offset from the
/// waveform's centre line.
#[must_use]
pub fn amp_to_half_height(amp: f64, height: f64) -> f64 {
    amp.clamp(0.0, 1.0) * height * 0.5
}

/// Evenly space `n` samples across the trace width.
///
/// With a single sample the point sits at the left edge; the caller draws a
/// degenerate path, which is correct for a ring that has only been written
/// once.
#[must_use]
pub fn sample_x(index: usize, n: usize, width: f64) -> f64 {
    if n <= 1 {
        return 0.0;
    }
    index as f64 / (n - 1) as f64 * width
}

/// Build the `d` of the gain-reduction area: a filled region hanging from the
/// top edge down to the reduction at each point.
///
/// Returns an empty string for an empty window so the caller can skip the
/// path element entirely.
#[must_use]
pub fn gr_area_path(gr_db: &[f32], width: f64, height: f64) -> String {
    if gr_db.is_empty() {
        return String::new();
    }
    let n = gr_db.len();
    let mut d = String::with_capacity(n * 16 + 32);
    d.push_str("M 0 0");
    for (i, &g) in gr_db.iter().enumerate() {
        let x = sample_x(i, n, width);
        let y = gr_to_y(f64::from(g), height);
        let _ = write!(d, " L {x:.2} {y:.2}");
    }
    // Close back along the top edge.
    let _ = write!(d, " L {width:.2} 0 Z");
    d
}

/// Build the `d` of the output waveform envelope: a symmetric filled shape
/// around the vertical centre of the trace.
#[must_use]
pub fn waveform_path(peaks: &[f32], width: f64, height: f64) -> String {
    if peaks.is_empty() {
        return String::new();
    }
    let n = peaks.len();
    let mid = height * 0.5;
    let mut d = String::with_capacity(n * 32 + 32);
    // Upper edge, left → right.
    for (i, &p) in peaks.iter().enumerate() {
        let x = sample_x(i, n, width);
        let y = mid - amp_to_half_height(f64::from(p), height);
        d.push_str(if i == 0 { "M " } else { " L " });
        let _ = write!(d, "{x:.2} {y:.2}");
    }
    // Lower edge, right → left.
    for (i, &p) in peaks.iter().enumerate().rev() {
        let x = sample_x(i, n, width);
        let y = mid + amp_to_half_height(f64::from(p), height);
        let _ = write!(d, " L {x:.2} {y:.2}");
    }
    d.push_str(" Z");
    d
}

/// Horizontal gridlines, as (y, label) pairs, every 6 dB of reduction.
#[must_use]
pub fn gr_gridlines(height: f64) -> Vec<(f64, String)> {
    let mut out = Vec::new();
    let mut db = 6.0;
    while db <= GR_RANGE_DB {
        out.push((gr_to_y(db, height), format!("-{db:.0}")));
        db += 6.0;
    }
    out
}
