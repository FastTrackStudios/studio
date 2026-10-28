//! A unit of work's stages, timed: [`Stages`].
//!
//! Natively, architect's timer: each mark lands on the current span for the
//! perf HUD and the wide event. A browser has no `std::time::Instant` (it
//! panics on wasm32-unknown-unknown) and no recorder to read the marks, so
//! there the same calls cost nothing and measure nothing.

#[cfg(not(target_arch = "wasm32"))]
pub use architect_telemetry::timing::Stages;

/// The browser's stand-in for architect's `Stages`: same calls, no clock.
#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
pub struct Stages(());

#[cfg(target_arch = "wasm32")]
impl Stages {
    /// Start timing (nothing, on the web).
    #[must_use]
    pub fn start() -> Self {
        Self(())
    }

    /// The time since the last mark, ms: none measured on the web.
    pub fn mark(&mut self, _key: &'static str) -> f64 {
        0.0
    }

    /// The time since the start, ms: none measured on the web.
    #[must_use]
    pub fn total_ms(&self) -> f64 {
        0.0
    }
}
