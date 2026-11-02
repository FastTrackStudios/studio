//! An in-memory [`SurfaceEngine`]: a mock engine for previews, tests and
//! designing surfaces without a running rig.
//!
//! It holds a [`Catalog`], stores parameter values, fans changes out to
//! subscribers, and synthesizes every feed kind ([`Memory::tick`], from
//! `feed-memory`'s generators) so meters, spectra, traces, scopes and
//! envelopes move in a preview — shaped by `request_feeds` options.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::surface::proto::*;
use architect::dispatch::CurrentThreadDispatcher;
use architect::{HasDispatcher, PubSub};

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Clone, HasDispatcher)]
#[dispatch(CurrentThreadDispatcher)]
pub struct Memory {
    catalog: Arc<Catalog>,
    values: Arc<Mutex<BTreeMap<ParamAddr, f64>>>,
    /// Text params' values (names), empty until set.
    texts: Arc<Mutex<BTreeMap<ParamAddr, String>>>,
    invoked: Arc<Mutex<Vec<EntryId>>>,
    selected: Arc<Mutex<BTreeMap<EntryId, u32>>>,
    /// Feed options by feed, as last requested.
    requested: Arc<Mutex<BTreeMap<EntryId, FeedOptions>>>,
    changes: PubSub<ParamValue>,
    telemetry: PubSub<StreamFrame>,
}

impl Memory {
    /// An engine exposing `catalog`, every parameter at its default.
    #[must_use]
    pub fn new(catalog: Catalog) -> Self {
        let values = catalog
            .params
            .iter()
            .map(|p| (p.addr.clone(), p.default))
            .collect();
        Self {
            catalog: Arc::new(catalog),
            values: Arc::new(Mutex::new(values)),
            texts: Arc::new(Mutex::new(BTreeMap::new())),
            invoked: Arc::new(Mutex::new(Vec::new())),
            selected: Arc::new(Mutex::new(BTreeMap::new())),
            requested: Arc::new(Mutex::new(BTreeMap::new())),
            changes: PubSub::sliding(256),
            telemetry: PubSub::sliding(64),
        }
    }

    /// The demo guitar rig ([`demo_catalog`]).
    #[must_use]
    pub fn demo() -> Self {
        Self::new(demo_catalog())
    }

    #[must_use]
    pub fn value(&self, addr: &ParamAddr) -> Option<f64> {
        lock(&self.values).get(addr).copied()
    }

    /// A text param's value (`None` for an unknown or non-text param).
    #[must_use]
    pub fn text(&self, addr: &ParamAddr) -> Option<String> {
        let info = self.catalog.param(addr)?;
        (info.kind == ParamKind::Text)
            .then(|| lock(&self.texts).get(addr).cloned().unwrap_or_default())
    }

    /// Actions fired so far, oldest first.
    #[must_use]
    pub fn invoked(&self) -> Vec<EntryId> {
        lock(&self.invoked).clone()
    }

    /// Every feed as currently shaped: its catalog entry resolved against
    /// the latest request for it.
    #[must_use]
    pub fn feeds(&self) -> Vec<FeedInfo> {
        let requested = lock(&self.requested);
        self.catalog
            .streams
            .iter()
            .map(|s| {
                requested
                    .get(&s.id)
                    .map_or_else(|| s.clone(), |o| s.resolve(o))
            })
            .collect()
    }

    /// One synthetic frame per feed at `t` (seconds), shaped as requested.
    #[must_use]
    pub fn frames(&self, t: f64) -> Vec<StreamFrame> {
        crate::feed::memory::frames(&self.feeds(), t)
    }

    /// Publish [`Self::frames`] at `t` (seconds), so previews move.
    pub fn tick(&self, t: f64) {
        for f in self.frames(t) {
            self.telemetry.publish(f);
        }
    }
}

impl SurfaceEngine for Memory {
    fn catalog(&self) -> Catalog {
        (*self.catalog).clone()
    }

    fn values(&self) -> Vec<ParamValue> {
        lock(&self.values)
            .iter()
            .map(|(addr, value)| ParamValue {
                addr: addr.clone(),
                value: *value,
            })
            .collect()
    }

    fn set_param(
        &self,
        addr: ParamAddr,
        value: f64,
        _phase: EditPhase,
    ) -> Result<(), SurfaceError> {
        let Some(info) = self.catalog.param(&addr) else {
            return Err(SurfaceError::UnknownParam(addr.0));
        };
        // Store what the engine would actually hold (stepped params snap).
        let value = info.range.to_normalized(info.range.to_real(value));
        lock(&self.values).insert(addr.clone(), value);
        self.changes.publish(ParamValue { addr, value });
        Ok(())
    }

    fn texts(&self) -> Vec<TextValue> {
        let texts = lock(&self.texts);
        self.catalog
            .params
            .iter()
            .filter(|p| p.kind == ParamKind::Text)
            .map(|p| TextValue {
                addr: p.addr.clone(),
                text: texts.get(&p.addr).cloned().unwrap_or_default(),
            })
            .collect()
    }

    fn set_text(&self, addr: ParamAddr, text: String) -> Result<(), SurfaceError> {
        match self.catalog.param(&addr) {
            None => Err(SurfaceError::UnknownParam(addr.0)),
            Some(p) if p.kind != ParamKind::Text => Err(SurfaceError::NotText(addr.0)),
            Some(_) => {
                lock(&self.texts).insert(addr, text);
                Ok(())
            }
        }
    }

    fn invoke(&self, action: EntryId) -> Result<(), SurfaceError> {
        if self.catalog.action(&action).is_none() {
            return Err(SurfaceError::UnknownAction(action.0));
        }
        lock(&self.invoked).push(action);
        Ok(())
    }

    fn select(&self, collection: EntryId, index: u32) -> Result<(), SurfaceError> {
        if self.catalog.collection(&collection).is_none() {
            return Err(SurfaceError::UnknownCollection(collection.0));
        }
        lock(&self.selected).insert(collection, index);
        Ok(())
    }

    fn request_feeds(&self, requests: Vec<FeedRequest>) -> Result<Vec<FeedInfo>, SurfaceError> {
        let infos = requests
            .iter()
            .map(|r| {
                let info = self
                    .catalog
                    .stream(&r.id)
                    .ok_or_else(|| SurfaceError::UnknownFeed(r.id.0.clone()))?;
                Ok(info.resolve(&r.options))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut requested = lock(&self.requested);
        for r in requests {
            requested.insert(r.id, r.options);
        }
        Ok(infos)
    }
}

impl SurfaceEngineStreamSource for Memory {
    fn param_changes_hub(&self) -> &PubSub<ParamValue> {
        &self.changes
    }

    fn telemetry_hub(&self) -> &PubSub<StreamFrame> {
        &self.telemetry
    }
}

impl architect::Services for Memory {
    fn layers() -> impl architect::Layer<Self> {
        architect::layers![
            crate::surface::proto::engine::Service,
            crate::surface::proto::engine::StreamService
        ]
    }
}

/// A small guitar rig: an amp, a drive, a delay, a reverb, stomp actions,
/// meters and a tuner — enough to design and test real surfaces against.
#[must_use]
pub fn demo_catalog() -> Catalog {
    let db = |min, max| Range::linear(min, max, Unit::Db);
    let pct = Range::linear(0.0, 100.0, Unit::Percent);
    let param = |addr: &str, name: &str, group: &str, range: Range, default: f64| {
        ParamInfo::new(addr, name, range)
            .in_group(group)
            .with_default_real(default)
    };
    Catalog {
        id: "demo.guitar".into(),
        version: 1,
        params: vec![
            param(
                "amp/gain",
                "Gain",
                "Amp",
                Range::linear(0.0, 10.0, Unit::None),
                5.0,
            ),
            param(
                "amp/bass",
                "Bass",
                "Amp",
                Range::linear(0.0, 10.0, Unit::None),
                5.0,
            ),
            param(
                "amp/mid",
                "Mid",
                "Amp",
                Range::linear(0.0, 10.0, Unit::None),
                5.0,
            ),
            param(
                "amp/treble",
                "Treble",
                "Amp",
                Range::linear(0.0, 10.0, Unit::None),
                5.0,
            ),
            param(
                "amp/presence",
                "Presence",
                "Amp",
                Range::linear(0.0, 10.0, Unit::None),
                5.0,
            ),
            param("amp/master", "Master", "Amp", db(-60.0, 6.0), -12.0),
            ParamInfo::new("amp/channel", "Channel", Range::default())
                .in_group("Amp")
                .choice(&["Clean", "Crunch", "Lead"]),
            param("drive/amount", "Drive", "Drive", pct.clone(), 40.0),
            param("drive/tone", "Tone", "Drive", pct.clone(), 50.0),
            ParamInfo::new("drive/on", "Drive On", Range::default())
                .in_group("Drive")
                .toggle()
                .short("Drive"),
            param(
                "delay/time",
                "Time",
                "Delay",
                Range::log(20.0, 2000.0, Unit::Milliseconds),
                350.0,
            ),
            param("delay/feedback", "Feedback", "Delay", pct.clone(), 30.0),
            param("delay/mix", "Mix", "Delay", pct.clone(), 20.0),
            ParamInfo::new("delay/on", "Delay On", Range::default())
                .in_group("Delay")
                .toggle()
                .short("Delay"),
            param(
                "reverb/decay",
                "Decay",
                "Reverb",
                Range::log(0.2, 10.0, Unit::Seconds),
                2.5,
            ),
            param(
                "reverb/tone",
                "Tone",
                "Reverb",
                Range::log(500.0, 12_000.0, Unit::Hz),
                4000.0,
            ),
            param("reverb/mix", "Mix", "Reverb", pct, 25.0),
            ParamInfo::new("reverb/on", "Reverb On", Range::default())
                .in_group("Reverb")
                .toggle()
                .short("Reverb"),
            param("rig/volume", "Volume", "Output", db(-60.0, 6.0), 0.0),
            ParamInfo {
                bipolar: true,
                ..param(
                    "rig/pan",
                    "Pan",
                    "Output",
                    Range::linear(-100.0, 100.0, Unit::Percent),
                    0.0,
                )
            },
        ],
        actions: vec![
            ActionInfo {
                id: EntryId::new("tap_tempo"),
                name: "Tap Tempo".into(),
                group: Some("Global".into()),
            },
            ActionInfo {
                id: EntryId::new("next_song"),
                name: "Next Song".into(),
                group: Some("Setlist".into()),
            },
            ActionInfo {
                id: EntryId::new("prev_song"),
                name: "Previous Song".into(),
                group: Some("Setlist".into()),
            },
            ActionInfo {
                id: EntryId::new("tuner"),
                name: "Tuner".into(),
                group: Some("Global".into()),
            },
        ],
        streams: vec![
            StreamInfo {
                id: EntryId::new("out_level"),
                name: "Output".into(),
                kind: StreamKind::Level { channels: 2 },
                rate_hz: 30.0,
            },
            StreamInfo {
                id: EntryId::new("in_level"),
                name: "Input".into(),
                kind: StreamKind::Level { channels: 1 },
                rate_hz: 30.0,
            },
            StreamInfo {
                id: EntryId::new("tuner"),
                name: "Tuner".into(),
                kind: StreamKind::Tuner,
                rate_hz: 30.0,
            },
            StreamInfo {
                id: EntryId::new("tempo"),
                name: "Tempo".into(),
                kind: StreamKind::Scalar {
                    range: Range::linear(40.0, 240.0, Unit::Bpm),
                },
                rate_hz: 4.0,
            },
        ],
        collections: vec![
            CollectionInfo {
                id: EntryId::new("songs"),
                name: "Songs".into(),
            },
            CollectionInfo {
                id: EntryId::new("presets"),
                name: "Presets".into(),
            },
        ],
    }
}
