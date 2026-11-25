//! The wheel, as the shared bindings see it — plus the one piece that
//! needs a dioxus type: turning a `WheelDelta` into notches.
//!
//! Everything else lives in `crate::paint::scroll` and is
//! re-exported here so the components keep their paths.

pub use crate::paint::scroll::*;

/// A wheel delta in notches, whatever unit it arrived in.
///
/// Normalising here rather than at each use means the gain constants
/// stay tuned for one unit, and a device that reports a new one is a
/// change in this function alone.
pub fn notches(delta: &dioxus::html::geometry::WheelDelta) -> (f64, f64) {
    use dioxus::html::geometry::WheelDelta;
    match delta {
        WheelDelta::Lines(v) => (v.x, v.y),
        WheelDelta::Pixels(v) => (v.x / PIXELS_PER_NOTCH, v.y / PIXELS_PER_NOTCH),
        // A page is a screenful; treat it as a firm push rather than
        // scaling it into the thousands.
        WheelDelta::Pages(v) => (v.x * 4.0, v.y * 4.0),
    }
}
