//! A span of the song rendered offline and measured: what MIX_MEASURE
//! (engine-listen's `mix_measure`) does, and what the mix's variants read.
//!
//! The measurement itself ([`analysis`]) is plain arithmetic on samples;
//! the render ([`render_span`]) is engine-listen's, reached through the
//! [`crate::assembly`] (it copies the mix's plugins).

pub mod analysis;

use std::collections::{HashMap, HashSet};

use daw::standalone::Standalone;

pub use analysis::{Bands, FLOOR, Stats, measure as analyse};

/// The rate everything is rendered and measured at.
pub const RATE: u32 = 48_000;

/// How a span is rendered.
pub struct Span<'a> {
    pub from: f64,
    pub to: f64,
    /// Tracks to tap, by GUID.
    pub tap: &'a HashSet<String>,
    pub post_fader: bool,
    pub bypass_fx: HashSet<String>,
    pub solo: Option<HashSet<String>>,
}

/// A span rendered: master and the tracks asked for, each exactly
/// `from`..`to`, interleaved stereo.
pub struct Rendered {
    pub master: Vec<f32>,
    /// By track GUID.
    pub tracks: HashMap<String, Vec<f32>>,
    /// Each tapped track's latency, samples, by GUID.
    pub latencies: HashMap<String, u32>,
    pub latency: u32,
    pub copied: u32,
    pub failures: Vec<String>,
    /// Copying the plugins, priming them, and rendering the blocks, ms.
    pub copy_ms: f64,
    pub prime_ms: f64,
    pub blocks_ms: f64,
    pub ms: f64,
}

/// Render `span` of song `project` offline (engine-listen's
/// `mix_measure::render_span`, through the assembly).
///
/// # Errors
/// The span is empty or too long, or no engine is installed around the
/// core.
pub fn render_span(daw: &Standalone, project: &str, span: &Span<'_>) -> Result<Rendered, String> {
    let assembly = crate::assembly::get().ok_or("no engine installed to render with")?;
    (assembly.render_span)(daw, project, span)
}
