//! What an engine exposes to surfaces: parameters, actions, telemetry
//! streams and collections — published as one [`Catalog`].
//!
//! Surfaces bind to catalog entries by stable address. A binding to an
//! address the catalog doesn't contain is a validation error, so neither a
//! person nor an AI can wire a control to something that doesn't exist.

use facet::Facet;
use serde::{Deserialize, Serialize};

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

pub use crate::feed::proto::{
    EntryId, FeedInfo, FeedKind, FormatMark, FormatStyle, Range, ScalarKind, Scale, Unit,
    ValueFormat, note_name,
};

/// A catalog's telemetry entry — a [`FeedInfo`]. The old name, kept so
/// existing engines and surfaces compile unchanged.
pub type StreamInfo = FeedInfo;
/// The kind of a telemetry entry — a [`FeedKind`] (same wire format).
pub type StreamKind = FeedKind;

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
    /// A string (a band's name, a scene title). Its value travels as text
    /// (`SurfaceEngine::texts` / `set_text`); the normalized value is unused.
    Text,
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
    /// What a performer reaches for (a block's main control, its bypass).
    /// Engines with large catalogs mark these so generated and AI-designed
    /// surfaces start from the controls that matter.
    #[serde(default)]
    #[facet(default)]
    pub featured: bool,
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
            featured: false,
        }
    }

    #[must_use]
    pub fn featured(mut self) -> Self {
        self.featured = true;
        self
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

    /// A text param (a name).
    #[must_use]
    pub fn text(mut self) -> Self {
        self.kind = ParamKind::Text;
        self
    }

    #[must_use]
    pub fn choice(mut self, options: &[&str]) -> Self {
        self.range.steps = u32::try_from(options.len()).ok();
        self.kind = ParamKind::Choice {
            options: options.iter().map(ToString::to_string).collect(),
        };
        self
    }

    /// The display string for a normalized value.
    #[must_use]
    pub fn display(&self, normalized: f64) -> String {
        self.display_as(normalized, None)
    }

    /// [`Self::display`] under a readout's own [`ValueFormat`]. A toggle or
    /// choice keeps its words unless the format names labels.
    #[must_use]
    pub fn display_as(&self, normalized: f64, format: Option<&ValueFormat>) -> String {
        if let Some(f) = format {
            match (&f.style, &self.kind) {
                (FormatStyle::Labels { labels }, _) if !labels.is_empty() => {
                    let last = labels.len() - 1;
                    let i = (normalized.clamp(0.0, 1.0) * last as f64).round() as usize;
                    return labels[i.min(last)].clone();
                }
                (_, ParamKind::Continuous) => {
                    return self.range.format_as(self.range.to_real(normalized), f);
                }
                _ => {}
            }
        }
        match &self.kind {
            ParamKind::Toggle => if normalized >= 0.5 { "On" } else { "Off" }.to_string(),
            ParamKind::Choice { options } if !options.is_empty() => {
                let last = options.len() - 1;
                let i = (normalized.clamp(0.0, 1.0) * last as f64).round() as usize;
                options[i.min(last)].clone()
            }
            ParamKind::Text => String::new(),
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

    /// The feed `id` (a telemetry stream) — [`Self::stream`] by its new name.
    #[must_use]
    pub fn feed(&self, id: &EntryId) -> Option<&FeedInfo> {
        self.stream(id)
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
