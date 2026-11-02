//! An in-memory [`SurfaceEngine`]: a mock engine for previews, tests and
//! designing surfaces without a running rig.
//!
//! It holds a [`Catalog`], stores parameter values, fans changes out to
//! subscribers, and synthesizes every feed kind ([`Memory::tick`], from
//! `feed-memory`'s generators) so meters, spectra, traces, scopes and
//! envelopes move in a preview — shaped by `request_feeds` options.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::engine::SurfaceEngineStreamSource;
use architect::dispatch::CurrentThreadDispatcher;
use architect::{HasDispatcher, PubSub};
use frame::surface::proto::*;

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

    /// The demo guitar rig ([`demo_catalog`](frame::surface::demo::demo_catalog)).
    #[must_use]
    pub fn demo() -> Self {
        Self::new(frame::surface::demo::demo_catalog())
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
        frame::feed::memory::frames(&self.feeds(), t)
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
        architect::layers![crate::engine::Service, crate::engine::StreamService]
    }
}
