//! Hit detection for recorded drum kits and rhythmic audio, with a swappable
//! detector registry (`DetectorRegistry`) so multiple algorithms (trigger+mic
//! fusion, envelope gate, third-party/ML detectors) share one canonical output.

#[cfg(feature = "onnx")]
mod adtof;
pub mod attack;
#[cfg(feature = "onnx")]
mod beat_this;
pub mod bleed;
pub mod calibrate;
pub mod fuse;
pub mod gate;
pub mod kit;
pub mod models;
#[cfg(feature = "onnx")]
pub mod onnx;
pub mod registry;
pub mod trigger;

pub use attack::{attack_peak, attack_start};
pub use calibrate::{Agreement, Calibration, agree, calibrate};
pub use fuse::{Hit, fuse, pair_flams};
pub use gate::{Gate, Scan, gate};
pub use kit::{Found, Method, Piece, Settings, Trust, detect_kit};
pub use models::{LicenseTier, ModelManifest, ModelSpec, default_cache_dir, resolve_cached_model};
pub use registry::{
    BeatEvent, DetectedHit, DetectorInfo, DetectorInput, DetectorOutput, DetectorRegistry,
    HitDetector, PieceInput, SectionEvent,
};
pub use trigger::{Health, health, spikes};

/// Seconds as a whole number of samples at `rate` (0 for anything negative or non-finite).
#[must_use]
pub fn samples(seconds: f64, rate: f64) -> usize {
    num_traits::ToPrimitive::to_usize(&(seconds * rate).round()).unwrap_or(0)
}

/// A sample index as seconds at `rate`.
#[must_use]
pub fn seconds(sample: usize, rate: f64) -> f64 {
    num_traits::ToPrimitive::to_f64(&sample).unwrap_or(0.0) / rate.max(1.0)
}

/// Decibels as a linear gain.
#[must_use]
pub fn gain(db: f64) -> f64 {
    10_f64.powf(db / 20.0)
}

/// A linear level in decibels, floored well under anything audible.
#[must_use]
pub fn db(level: f64) -> f64 {
    20.0 * level.max(1e-9).log10()
}
