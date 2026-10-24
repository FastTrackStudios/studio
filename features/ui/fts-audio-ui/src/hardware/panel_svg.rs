//! How a fixed-aspect faceplate fits into a resizable editor window.
//!
//! A hardware panel is a physical object: the rack ears, the VU cutout and the
//! knob spacing are one drawing, and reflowing them the way a web layout
//! reflows is exactly the thing that stops it looking like a unit. So the
//! panel keeps its design size and is drawn at a single scale factor instead —
//! which also sidesteps blitz's habit of collapsing what does not fit rather
//! than clipping it, since at any scale the panel fits by construction.
//!
//! Geometry only; the components multiply their pixel sizes by
//! [`fit_scale`].

/// Smallest scale a panel is drawn at. The panel tracks the window all the
/// way down — a resizable editor that stops shrinking and crops instead is
/// broken resizing, not legibility protection. The floor only guards the
/// degenerate zero-size window.
pub const MIN_SCALE: f64 = 0.1;
/// Largest scale. Past roughly twice the design size a faceplate is just a
/// blurry enlargement, and the extra window space is better left as margin.
pub const MAX_SCALE: f64 = 2.0;

/// The scale factor that fits a `design_w` x `design_h` panel inside a
/// `win_w` x `win_h` window without distorting it.
///
/// One factor for both axes — a panel stretched on one axis reads as a
/// rendering bug, not as a bigger unit.
pub fn fit_scale(win_w: f64, win_h: f64, design_w: f64, design_h: f64) -> f64 {
    if design_w <= 0.0 || design_h <= 0.0 {
        return 1.0;
    }
    let s = (win_w / design_w).min(win_h / design_h);
    if !s.is_finite() {
        return 1.0;
    }
    s.clamp(MIN_SCALE, MAX_SCALE)
}
