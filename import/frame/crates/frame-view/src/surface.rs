//! The surface `frame-surface` shows: a Frame document (a `.frame` file,
//! or [`DOCUMENT`] until one is loaded), on Frame's own embed runtime,
//! painted into an [`Svg`].
//!
//! Its catalog is the document's ([`catalog_of`]): each control bound to a
//! param makes one (0 to 100 %, named by the control), each control bound
//! to a feed makes one (a level, dB). So the document is all a host needs
//! to know what it can move; it moves it with [`Live::set_param`] and
//! [`Live::feed`], and hears the knobs turned from [`Live::pointer`].

use std::collections::BTreeMap;

use frame::document::proto::model::Scene;
use frame::embed::{DocumentView, Embed, EmbedHost, EngineCall, Pointer};
use frame::surface::proto::{
    Catalog, EntryId, FeedKind, ParamAddr, ParamInfo, ParamValue, Range, Source, StreamFrame,
    StreamInfo, Unit,
};
use frame::surface::runtime::Surface;
use kurbo::Affine;
use serde_json::{Value, json};

use crate::svg::{Svg, SvgCanvas};

/// The document shown until one is loaded: a knob bound to
/// `surface/drive`, a meter to the `level` feed.
pub const DOCUMENT: &str = include_str!("../surfaces/knob-meter.frame");

/// A level feed's span, dB: the runtime's default scale, so a meter
/// reading `FLOOR_DB` is empty and `CEILING_DB` full.
pub const FLOOR_DB: f64 = -60.0;
pub const CEILING_DB: f64 = 6.0;

/// How long between two feed readings, ms: long enough that the last
/// one's bar and peak have fallen away, so a meter shows exactly the new
/// one.
const READING_MS: f64 = 1_000_000.0;

/// The document's catalog: a param per param-bound control and a feed per
/// feed-bound one, in the order they are drawn.
#[must_use]
pub fn catalog_of(scene: &Scene) -> Catalog {
    let mut params: Vec<ParamInfo> = Vec::new();
    let mut streams: Vec<StreamInfo> = Vec::new();
    for node in scene
        .walk(scene.root)
        .into_iter()
        .filter_map(|id| scene.node(id))
    {
        let Some(binding) = node.control.as_ref().and_then(|c| c.binding.as_ref()) else {
            continue;
        };
        match &binding.source {
            Source::Param { addr } if !params.iter().any(|p| p.addr == *addr) => params.push(
                ParamInfo::new(
                    addr.0.clone(),
                    node.name.clone(),
                    Range::linear(0.0, 100.0, Unit::Percent),
                )
                .with_default_real(50.0),
            ),
            Source::Stream { id, .. } if !streams.iter().any(|s| s.id == *id) => {
                streams.push(StreamInfo::new(
                    id.0.clone(),
                    node.name.clone(),
                    FeedKind::Level { channels: 1 },
                ));
            }
            _ => {}
        }
    }
    Catalog {
        id: crate::KIND.to_owned(),
        version: 1,
        params,
        streams,
        ..Catalog::default()
    }
}

/// A param as a host sees it: its address, its control's name, where it
/// is (0 to 1).
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub addr: String,
    pub name: String,
    pub value: f64,
}

/// A feed as a host sees it: its id, its control's name, its last reading
/// (dB, a value per channel).
#[derive(Debug, Clone, PartialEq)]
pub struct Feed {
    pub id: String,
    pub name: String,
    pub values: Vec<f64>,
}

/// A document's surface, laid out `width` px wide, and where its params
/// and feeds are.
pub struct Live {
    embed: Embed,
    width: f64,
    /// Each param, 0..1; kept across a reload for the params that stay.
    params: BTreeMap<String, f64>,
    /// Each feed's last reading; kept across a reload too.
    feeds: BTreeMap<String, Vec<f64>>,
    /// When the last reading was, ms.
    t: f64,
}

impl std::fmt::Debug for Live {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Live")
            .field("params", &self.params)
            .field("feeds", &self.feeds)
            .field("size", &self.embed.size())
            .finish_non_exhaustive()
    }
}

fn embed(text: &str, width: f64) -> Result<Embed, String> {
    let scene = frame::document::text::parse(text, BTreeMap::new()).map_err(|e| e.to_string())?;
    let catalog = catalog_of(&scene);
    let view = DocumentView::new(Surface::from_scene(scene), catalog, EmbedHost::Strip);
    let mut embed = Embed::new(view, EmbedHost::Strip);
    let height = embed.height_for(width).unwrap_or(width);
    embed.set_size(width, height);
    Ok(embed)
}

fn unknown<'a>(what: &str, name: &str, known: impl Iterator<Item = &'a String>) -> String {
    let known: Vec<&str> = known.map(String::as_str).collect();
    let known = if known.is_empty() {
        "none".to_owned()
    } else {
        known.join(", ")
    };
    format!("the surface has no {what} `{name}`; its {what}s: {known}")
}

impl Live {
    /// The document `text`'s surface at `width` px, each param at its
    /// default and each feed at its floor.
    ///
    /// # Errors
    ///
    /// The document does not parse.
    pub fn new(text: &str, width: f64) -> Result<Self, String> {
        let mut live = Self {
            embed: embed(text, width)?,
            width,
            params: BTreeMap::new(),
            feeds: BTreeMap::new(),
            t: 0.0,
        };
        live.adopt();
        Ok(live)
    }

    /// Shows the document `text` instead, its params and feeds where they
    /// were for the ones it still has.
    ///
    /// # Errors
    ///
    /// The document does not parse; the surface stays as it was.
    pub fn reload(&mut self, text: &str) -> Result<(), String> {
        self.embed = embed(text, self.width)?;
        self.adopt();
        Ok(())
    }

    /// Takes the embed's catalog, keeps the params and feeds it still has
    /// where they were, and puts them all on the surface.
    fn adopt(&mut self) {
        let catalog = self.embed.catalog().clone();
        self.params = catalog
            .params
            .iter()
            .map(|p| {
                let was = self.params.get(&p.addr.0).copied();
                (p.addr.0.clone(), was.unwrap_or(p.default))
            })
            .collect();
        self.feeds = catalog
            .streams
            .iter()
            .map(|s| {
                let was = self.feeds.get(&s.id.0).cloned();
                (s.id.0.clone(), was.unwrap_or_else(|| vec![FLOOR_DB]))
            })
            .collect();
        self.embed
            .apply_values(self.params.iter().map(|(addr, value)| ParamValue {
                addr: ParamAddr::new(addr.clone()),
                value: *value,
            }));
        for (id, values) in self.feeds.clone() {
            self.reading(&id, values);
        }
    }

    fn reading(&mut self, id: &str, values: Vec<f64>) {
        self.t += READING_MS;
        self.embed.apply_frame(
            StreamFrame {
                id: EntryId::new(id),
                values,
            },
            self.t,
        );
    }

    /// Its params, in the document's order.
    #[must_use]
    pub fn params(&self) -> Vec<Param> {
        self.embed
            .catalog()
            .params
            .iter()
            .map(|p| Param {
                addr: p.addr.0.clone(),
                name: p.name.clone(),
                value: self.params.get(&p.addr.0).copied().unwrap_or_default(),
            })
            .collect()
    }

    /// Its feeds, in the document's order.
    #[must_use]
    pub fn feeds(&self) -> Vec<Feed> {
        self.embed
            .catalog()
            .streams
            .iter()
            .map(|s| Feed {
                id: s.id.0.clone(),
                name: s.name.clone(),
                values: self.feeds.get(&s.id.0).cloned().unwrap_or_default(),
            })
            .collect()
    }

    /// Where the param `addr` is, 0..1.
    #[must_use]
    pub fn param(&self, addr: &str) -> Option<f64> {
        self.params.get(addr).copied()
    }

    /// The param `addr` as its control shows it (its engineering units).
    #[must_use]
    pub fn param_real(&self, addr: &str) -> Option<f64> {
        self.embed.value(addr)
    }

    /// Moves the param `addr` (clamped to 0..1).
    ///
    /// # Errors
    ///
    /// The document has no such param.
    pub fn set_param(&mut self, addr: &str, value: f64) -> Result<(), String> {
        let Some(slot) = self.params.get_mut(addr) else {
            return Err(unknown("param", addr, self.params.keys()));
        };
        *slot = value.clamp(0.0, 1.0);
        let value = *slot;
        self.embed.apply_values([ParamValue {
            addr: ParamAddr::new(addr),
            value,
        }]);
        Ok(())
    }

    /// A reading of the feed `id`: dB, a value per channel, each clamped
    /// to the meter's span.
    ///
    /// # Errors
    ///
    /// The document has no such feed, or the reading has no value.
    pub fn feed(&mut self, id: &str, values: &[f64]) -> Result<(), String> {
        if !self.feeds.contains_key(id) {
            return Err(unknown("feed", id, self.feeds.keys()));
        }
        if values.is_empty() {
            return Err(format!(
                "a reading of `{id}` has a value per channel, not none"
            ));
        }
        let values: Vec<f64> = values
            .iter()
            .map(|v| v.clamp(FLOOR_DB, CEILING_DB))
            .collect();
        self.feeds.insert(id.to_owned(), values.clone());
        self.reading(id, values);
        Ok(())
    }

    /// Pointer input (surface-local px): the params it moved, each where it
    /// is now.
    pub fn pointer(&mut self, event: Pointer) -> Vec<(String, f64)> {
        let mut calls = self.embed.pointer(event, self.t).calls;
        calls.extend(self.embed.poll(self.t));
        let mut moved: Vec<(String, f64)> = Vec::new();
        for call in calls {
            if let EngineCall::SetParam { addr, value, .. } = call
                && let Some(slot) = self.params.get_mut(&addr.0)
            {
                *slot = value.clamp(0.0, 1.0);
                let value = *slot;
                self.embed.apply_values([ParamValue {
                    addr: addr.clone(),
                    value,
                }]);
                moved.retain(|(a, _)| *a != addr.0);
                moved.push((addr.0, value));
            }
        }
        moved
    }

    /// Its size, px.
    #[must_use]
    pub fn size(&self) -> (f64, f64) {
        self.embed.size()
    }

    /// The surface painted now.
    pub fn svg(&mut self) -> Svg {
        let (w, h) = self.embed.size();
        let mut canvas = SvgCanvas::new();
        self.embed.paint(&mut canvas, Affine::IDENTITY, self.t);
        canvas.finish(w, h)
    }

    /// Its params (`{addr, name, value}`, value 0..1) and feeds
    /// (`{id, name, values}`, dB), as an action answers them.
    #[must_use]
    pub fn json(&self) -> Value {
        let params: Vec<Value> = self
            .params()
            .into_iter()
            .map(|p| json!({ "addr": p.addr, "name": p.name, "value": p.value }))
            .collect();
        let feeds: Vec<Value> = self
            .feeds()
            .into_iter()
            .map(|f| json!({ "id": f.id, "name": f.name, "values": f.values }))
            .collect();
        json!({ "params": params, "feeds": feeds })
    }
}
