//! The params and feeds the visualizers bind to, by namespace — for
//! engines to publish (or mock engines to serve) exactly what a placed
//! `viz.*` component expects.

use surface_proto::{FeedInfo, FeedKind, ParamInfo, Range, ScalarKind, Unit};

/// EQ band shapes, in the option order `eq.response` reads.
pub const SHAPES: [&str; 7] = ["Bell", "Low Shelf", "High Shelf", "Low Cut", "High Cut", "Notch", "Band Pass"];

/// A starting point per band: (freq Hz, gain dB, Q, shape index).
const BAND_DEFAULTS: [(f64, f64, f64, usize); 8] = [
    (40.0, 0.0, 0.71, 3),
    (120.0, 3.0, 0.71, 1),
    (420.0, -4.0, 1.4, 0),
    (1800.0, 2.0, 1.0, 0),
    (4200.0, 5.0, 0.9, 0),
    (9000.0, 2.5, 0.71, 2),
    (15_000.0, 0.0, 0.71, 4),
    (6000.0, 0.0, 4.0, 5),
];

/// `<ns>/b{n}_{freq,gain,q,shape,on}` for bands `1..=bands`.
#[must_use]
pub fn eq_params(ns: &str, bands: usize) -> Vec<ParamInfo> {
    let mut out = Vec::new();
    for n in 1..=bands {
        let (f, g, q, shape) = BAND_DEFAULTS[(n - 1) % BAND_DEFAULTS.len()];
        let group = format!("Band {n}");
        let p = |k: &str, name: &str, range: Range| ParamInfo::new(format!("{ns}/b{n}_{k}"), format!("{group} {name}"), range).in_group(group.clone());
        out.push(p("freq", "Freq", Range::log(20.0, 20_000.0, Unit::Hz)).with_default_real(f).short("Freq"));
        out.push(p("gain", "Gain", Range::linear(-24.0, 24.0, Unit::Db)).with_default_real(g).short("Gain").bipolar());
        out.push(p("q", "Q", Range::log(0.1, 18.0, Unit::None)).with_default_real(q).short("Q"));
        let mut shape_p = p("shape", "Shape", Range::default()).choice(&SHAPES).short("Shape");
        shape_p.default = shape as f64 / (SHAPES.len() - 1) as f64;
        out.push(shape_p);
        let mut on = p("on", "On", Range::default()).toggle().short("On");
        on.default = 1.0;
        out.push(on);
    }
    out
}

/// `<ns>/spectrum`: `bins` log-spaced bins in dB over 20 Hz–20 kHz.
#[must_use]
pub fn spectrum_stream(ns: &str, bins: u16) -> FeedInfo {
    FeedInfo::new(format!("{ns}/spectrum"), "Spectrum", FeedKind::spectrum(bins))
}

/// `<ns>/{threshold,ratio,knee,makeup}`.
#[must_use]
pub fn comp_params(ns: &str) -> Vec<ParamInfo> {
    let p = |k: &str, name: &str, range: Range, v: f64| ParamInfo::new(format!("{ns}/{k}"), name, range).in_group("Compressor").with_default_real(v);
    vec![
        p("threshold", "Threshold", Range::linear(-60.0, 0.0, Unit::Db), -24.0).featured(),
        p("ratio", "Ratio", Range::log(1.0, 20.0, Unit::Ratio), 4.0).featured(),
        p("knee", "Knee", Range::linear(0.0, 24.0, Unit::Db), 6.0),
        p("makeup", "Makeup", Range::linear(0.0, 24.0, Unit::Db), 0.0),
    ]
}

/// A compressor's feeds: `<ns>/in` (input level) and `<ns>/gr` (gain
/// reduction) as readings, and `<ns>/in_history` / `<ns>/gr_history`, the
/// last `len` of each.
#[must_use]
pub fn comp_streams(ns: &str, len: u32) -> Vec<FeedInfo> {
    vec![
        FeedInfo::new(format!("{ns}/in"), "Input", FeedKind::Level { channels: 1 }),
        FeedInfo::new(format!("{ns}/gr"), "Gain Reduction", FeedKind::GainReduction),
        FeedInfo::new(format!("{ns}/in_history"), "Input History", FeedKind::History { of: ScalarKind::Level, len }).rate(60.0),
        FeedInfo::new(format!("{ns}/gr_history"), "GR History", FeedKind::gr_history(len)).rate(60.0),
    ]
}

/// `<ns>/{attack,decay,sustain,release}`: seconds, and a 0..1 level.
#[must_use]
pub fn adsr_params(ns: &str) -> Vec<ParamInfo> {
    let p = |k: &str, name: &str, range: Range, v: f64| ParamInfo::new(format!("{ns}/{k}"), name, range).in_group("Envelope").with_default_real(v);
    vec![
        p("attack", "Attack", Range::log(0.001, 2.0, Unit::Seconds), 0.08),
        p("decay", "Decay", Range::log(0.005, 4.0, Unit::Seconds), 0.45),
        p("sustain", "Sustain", Range::linear(0.0, 1.0, Unit::None), 0.6),
        p("release", "Release", Range::log(0.005, 6.0, Unit::Seconds), 1.1),
    ]
}

/// `<ns>/voices`: an ADSR playhead per voice (`[stage, position, level]`).
#[must_use]
pub fn adsr_stream(ns: &str, voices: u8) -> FeedInfo {
    FeedInfo::new(format!("{ns}/voices"), "Voices", FeedKind::adsr(voices)).rate(60.0)
}
