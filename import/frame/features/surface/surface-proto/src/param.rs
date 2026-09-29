//! What an engine exposes to surfaces: parameters, actions, telemetry
//! streams and collections — published as one [`Catalog`].
//!
//! Surfaces bind to catalog entries by stable address. A binding to an
//! address the catalog doesn't contain is a validation error, so neither a
//! person nor an AI can wire a control to something that doesn't exist.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::curve::Curve;

/// A parameter's stable address: `/`-separated path segments, owner first
/// (`guitar/amp/gain`, `keys/layer-1/filter/cutoff`). Addresses survive
/// save/load and renames of display names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Facet)]
#[facet(transparent)]
#[serde(transparent)]
pub struct ParamAddr(pub String);

impl ParamAddr {
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    /// signal's `ParamTarget { block_id, param_id }` shape.
    #[must_use]
    pub fn block(block_id: &str, param_id: &str) -> Self {
        Self(format!("{block_id}/{param_id}"))
    }

    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('/').filter(|s| !s.is_empty())
    }

    /// The owning scope (everything but the last segment).
    #[must_use]
    pub fn scope(&self) -> &str {
        self.0.rsplit_once('/').map_or("", |(s, _)| s)
    }
}

impl std::fmt::Display for ParamAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Identity of an action, stream or collection in a catalog.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Facet)]
#[facet(transparent)]
#[serde(transparent)]
pub struct EntryId(pub String);

impl EntryId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum Unit {
    #[default]
    None,
    Hz,
    Db,
    Seconds,
    Milliseconds,
    Percent,
    Ratio,
    Semitones,
    Cents,
    Degrees,
    Beats,
    Bpm,
    Custom { suffix: String },
}

impl Unit {
    #[must_use]
    pub fn suffix(&self) -> &str {
        match self {
            Self::None => "",
            Self::Hz => "Hz",
            Self::Db => "dB",
            Self::Seconds => "s",
            Self::Milliseconds => "ms",
            Self::Percent => "%",
            Self::Ratio => ":1",
            Self::Semitones => "st",
            Self::Cents => "ct",
            Self::Degrees => "°",
            Self::Beats => "beats",
            Self::Bpm => "BPM",
            Self::Custom { suffix } => suffix,
        }
    }
}

/// How the normalized 0..1 control position spreads over `min..max`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum Scale {
    /// Even across the range (dB gain, mix, pan).
    #[default]
    Linear,
    /// Even in ratio: equal travel is equal multiplication (frequency,
    /// time). Needs `min > 0`; otherwise behaves linearly.
    Logarithmic,
    /// Any other shape.
    Curve { curve: Curve },
}

/// A parameter's engineering range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Range {
    pub min: f64,
    pub max: f64,
    pub scale: Scale,
    pub unit: Unit,
    /// Whole steps only (modes, algorithm choices): `Some(n)` values.
    pub steps: Option<u32>,
}

impl Default for Range {
    /// The unit interval: normalized in, normalized out.
    fn default() -> Self {
        Self { min: 0.0, max: 1.0, scale: Scale::Linear, unit: Unit::None, steps: None }
    }
}

impl Range {
    #[must_use]
    pub fn linear(min: f64, max: f64, unit: Unit) -> Self {
        Self { min, max, scale: Scale::Linear, unit, steps: None }
    }

    #[must_use]
    pub fn log(min: f64, max: f64, unit: Unit) -> Self {
        Self { min, max, scale: Scale::Logarithmic, unit, steps: None }
    }

    fn quantize(&self, n: f64) -> f64 {
        match self.steps {
            Some(s) if s >= 2 => {
                let k = f64::from(s - 1);
                (n * k).round() / k
            }
            _ => n,
        }
    }

    /// Control position (0..1) → engineering value.
    #[must_use]
    pub fn to_real(&self, normalized: f64) -> f64 {
        let n = self.quantize(if normalized.is_nan() { 0.0 } else { normalized.clamp(0.0, 1.0) });
        match &self.scale {
            Scale::Logarithmic if self.min > 0.0 && self.max > 0.0 => self.min * (self.max / self.min).powf(n),
            Scale::Curve { curve } => self.min + (self.max - self.min) * curve.apply(n),
            _ => self.min + (self.max - self.min) * n,
        }
    }

    /// Engineering value → control position (0..1).
    #[must_use]
    pub fn to_normalized(&self, real: f64) -> f64 {
        let span = self.max - self.min;
        let n = match &self.scale {
            Scale::Logarithmic if self.min > 0.0 && self.max > 0.0 && real > 0.0 => {
                (real / self.min).ln() / (self.max / self.min).ln()
            }
            Scale::Curve { curve } if span != 0.0 => curve.invert((real - self.min) / span),
            _ if span != 0.0 => (real - self.min) / span,
            _ => 0.0,
        };
        self.quantize(n.clamp(0.0, 1.0))
    }

    /// Human readout: `"440 Hz"`, `"-6.0 dB"`, `"2.50 s"`, `"35 %"`.
    #[must_use]
    pub fn format(&self, real: f64) -> String {
        let (value, unit) = match self.unit {
            Unit::Hz if real.abs() >= 1000.0 => (real / 1000.0, "kHz".to_string()),
            Unit::Seconds if real.abs() < 1.0 => (real * 1000.0, "ms".to_string()),
            ref u => (real, u.suffix().to_string()),
        };
        let text = if self.steps.is_some() {
            format!("{value:.0}")
        } else if value.abs() >= 100.0 {
            format!("{value:.0}")
        } else if value.abs() >= 10.0 {
            format!("{value:.1}")
        } else {
            format!("{value:.2}")
        };
        if unit.is_empty() { text } else { format!("{text} {unit}") }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum ParamKind {
    /// A continuous value (knob, fader).
    #[default]
    Continuous,
    /// On/off.
    Toggle,
    /// One of named options; the value is the option index spread over 0..1.
    Choice { options: Vec<String> },
}

/// One parameter an engine exposes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ParamInfo {
    pub addr: ParamAddr,
    pub name: String,
    /// Short name for small readouts ("Cutoff" → "Cut").
    pub short_name: Option<String>,
    /// Grouping for browsers and layout templates ("Amp", "Filter").
    pub group: Option<String>,
    pub kind: ParamKind,
    pub range: Range,
    /// Default, normalized.
    pub default: f64,
    /// Neutral at center (pan, bipolar macros).
    pub bipolar: bool,
}

impl ParamInfo {
    #[must_use]
    pub fn new(addr: impl Into<String>, name: impl Into<String>, range: Range) -> Self {
        Self {
            addr: ParamAddr::new(addr),
            name: name.into(),
            short_name: None,
            group: None,
            kind: ParamKind::Continuous,
            range,
            default: 0.0,
            bipolar: false,
        }
    }

    #[must_use]
    pub fn with_default_real(mut self, real: f64) -> Self {
        self.default = self.range.to_normalized(real);
        self
    }

    #[must_use]
    pub fn short(mut self, short_name: impl Into<String>) -> Self {
        self.short_name = Some(short_name.into());
        self
    }

    #[must_use]
    pub fn in_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    #[must_use]
    pub fn bipolar(mut self) -> Self {
        self.bipolar = true;
        self
    }

    #[must_use]
    pub fn toggle(mut self) -> Self {
        self.kind = ParamKind::Toggle;
        self.range.steps = Some(2);
        self
    }

    #[must_use]
    pub fn choice(mut self, options: &[&str]) -> Self {
        self.range.steps = u32::try_from(options.len()).ok();
        self.kind = ParamKind::Choice { options: options.iter().map(ToString::to_string).collect() };
        self
    }

    /// The display string for a normalized value.
    #[must_use]
    pub fn display(&self, normalized: f64) -> String {
        match &self.kind {
            ParamKind::Toggle => if normalized >= 0.5 { "On" } else { "Off" }.to_string(),
            ParamKind::Choice { options } if !options.is_empty() => {
                let last = options.len() - 1;
                let i = (normalized.clamp(0.0, 1.0) * last as f64).round() as usize;
                options[i.min(last)].clone()
            }
            _ => self.range.format(self.range.to_real(normalized)),
        }
    }
}

/// Something a surface can trigger (`tap_tempo`, `next_song`, `press_stack`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ActionInfo {
    pub id: EntryId,
    pub name: String,
    pub group: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum StreamKind {
    /// Peak/RMS level in dBFS, per channel.
    Level { channels: u8 },
    /// Gain reduction in dB (≤ 0).
    GainReduction,
    /// Magnitude spectrum in dB over `bins` log-spaced bins.
    Spectrum { bins: u16 },
    /// Detected pitch: note, cents offset.
    Tuner,
    /// Any scalar readout (tempo, CPU %).
    Scalar { range: Range },
}

/// Read-only telemetry a surface can display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct StreamInfo {
    pub id: EntryId,
    pub name: String,
    pub kind: StreamKind,
    /// Nominal publish rate.
    pub rate_hz: f64,
}

/// A browsable, selectable list (songs, patches, stacks, presets).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct CollectionInfo {
    pub id: EntryId,
    pub name: String,
}

/// Everything one engine (rig, plugin, app) exposes to surfaces.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Facet)]
pub struct Catalog {
    /// Which engine (`signal.guitar`), for matching surfaces to engines.
    pub id: String,
    /// Bumped whenever addresses are added/removed/renamed.
    pub version: u32,
    pub params: Vec<ParamInfo>,
    pub actions: Vec<ActionInfo>,
    pub streams: Vec<StreamInfo>,
    pub collections: Vec<CollectionInfo>,
}

impl Catalog {
    #[must_use]
    pub fn param(&self, addr: &ParamAddr) -> Option<&ParamInfo> {
        self.params.iter().find(|p| &p.addr == addr)
    }

    #[must_use]
    pub fn action(&self, id: &EntryId) -> Option<&ActionInfo> {
        self.actions.iter().find(|a| &a.id == id)
    }

    #[must_use]
    pub fn stream(&self, id: &EntryId) -> Option<&StreamInfo> {
        self.streams.iter().find(|s| &s.id == id)
    }

    #[must_use]
    pub fn collection(&self, id: &EntryId) -> Option<&CollectionInfo> {
        self.collections.iter().find(|c| &c.id == id)
    }

    /// Parameters grouped by `group`, in catalog order (ungrouped last).
    #[must_use]
    pub fn groups(&self) -> Vec<(Option<&str>, Vec<&ParamInfo>)> {
        let mut out: Vec<(Option<&str>, Vec<&ParamInfo>)> = Vec::new();
        for p in &self.params {
            let g = p.group.as_deref();
            match out.iter_mut().find(|(k, _)| *k == g) {
                Some((_, v)) => v.push(p),
                None => out.push((g, vec![p])),
            }
        }
        out.sort_by_key(|(g, _)| g.is_none());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_range_round_trips() {
        let r = Range::log(20.0, 20_000.0, Unit::Hz);
        assert!((r.to_real(0.0) - 20.0).abs() < 1e-9);
        assert!((r.to_real(1.0) - 20_000.0).abs() < 1e-6);
        assert!((r.to_real(0.5) - 632.455_532).abs() < 1e-3, "geometric mean at center");
        for i in 0..=10 {
            let n = f64::from(i) / 10.0;
            assert!((r.to_normalized(r.to_real(n)) - n).abs() < 1e-9);
        }
    }

    #[test]
    fn log_with_zero_min_falls_back_to_linear() {
        let r = Range::log(0.0, 10.0, Unit::Seconds);
        assert_eq!(r.to_real(0.5), 5.0);
    }

    #[test]
    fn formatting() {
        assert_eq!(Range::log(20.0, 20_000.0, Unit::Hz).format(440.0), "440 Hz");
        assert_eq!(Range::log(20.0, 20_000.0, Unit::Hz).format(2500.0), "2.50 kHz");
        assert_eq!(Range::linear(-60.0, 12.0, Unit::Db).format(-6.0), "-6.00 dB");
        assert_eq!(Range::linear(0.0, 10.0, Unit::Seconds).format(0.25), "250 ms");
    }

    #[test]
    fn choice_and_toggle_display() {
        let p = ParamInfo::new("amp/voice", "Voice", Range::default()).choice(&["Clean", "Crunch", "Lead"]);
        assert_eq!(p.display(0.0), "Clean");
        assert_eq!(p.display(0.5), "Crunch");
        assert_eq!(p.display(1.0), "Lead");
        let t = ParamInfo::new("fx/bypass", "Bypass", Range::default()).toggle();
        assert_eq!(t.display(1.0), "On");
    }

    #[test]
    fn stepped_ranges_quantize() {
        let r = Range { steps: Some(5), ..Range::linear(0.0, 4.0, Unit::None) };
        assert_eq!(r.to_real(0.3), 1.0);
        assert_eq!(r.to_normalized(2.9), 0.75);
    }

    #[test]
    fn addr_helpers() {
        let a = ParamAddr::new("keys/layer-1/filter/cutoff");
        assert_eq!(a.scope(), "keys/layer-1/filter");
        assert_eq!(a.segments().count(), 4);
        assert_eq!(ParamAddr::block("amp", "gain").0, "amp/gain");
    }
}
