//! An in-memory [`SurfaceEngine`]: a mock engine for previews, tests and
//! designing surfaces without a running rig.
//!
//! It holds a [`Catalog`], stores parameter values, fans changes out to
//! subscribers, and can synthesize telemetry ([`Memory::tick`]) so meters
//! move in a preview.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use architect::PubSub;
use surface_proto::*;

#[derive(Clone)]
pub struct Memory {
    catalog: Arc<Catalog>,
    values: Arc<Mutex<BTreeMap<ParamAddr, f64>>>,
    invoked: Arc<Mutex<Vec<EntryId>>>,
    selected: Arc<Mutex<BTreeMap<EntryId, u32>>>,
    changes: PubSub<ParamValue>,
    telemetry: PubSub<StreamFrame>,
}

impl Memory {
    /// An engine exposing `catalog`, every parameter at its default.
    #[must_use]
    pub fn new(catalog: Catalog) -> Self {
        let values = catalog.params.iter().map(|p| (p.addr.clone(), p.default)).collect();
        Self {
            catalog: Arc::new(catalog),
            values: Arc::new(Mutex::new(values)),
            invoked: Arc::new(Mutex::new(Vec::new())),
            selected: Arc::new(Mutex::new(BTreeMap::new())),
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
        architect::lock(&self.values).get(addr).copied()
    }

    /// Actions fired so far, oldest first.
    #[must_use]
    pub fn invoked(&self) -> Vec<EntryId> {
        architect::lock(&self.invoked).clone()
    }

    /// Publish one synthetic frame per stream, as a function of time `t`
    /// (seconds), so previews have moving meters.
    pub fn tick(&self, t: f64) {
        for s in &self.catalog.streams {
            let values = match &s.kind {
                StreamKind::Level { channels } => (0..*channels)
                    .map(|c| -18.0 + 12.0 * (t * 2.3 + f64::from(c) * 0.7).sin() - 6.0 * (t * 0.37).cos().abs())
                    .collect(),
                StreamKind::GainReduction => vec![-3.0 - 3.0 * (t * 1.7).sin().abs()],
                StreamKind::Spectrum { bins } => {
                    (0..*bins).map(|b| -30.0 - 0.3 * f64::from(b) + 6.0 * (t * 3.0 + f64::from(b) * 0.2).sin()).collect()
                }
                StreamKind::Tuner => vec![64.0, 8.0 * (t * 0.9).sin()],
                StreamKind::Scalar { range } => vec![range.to_real(0.5 + 0.5 * (t * 0.5).sin())],
            };
            self.telemetry.publish(StreamFrame { id: s.id.clone(), values });
        }
    }
}

impl SurfaceEngine for Memory {
    async fn catalog(&self) -> Result<Catalog, SurfaceError> {
        Ok((*self.catalog).clone())
    }

    async fn values(&self) -> Result<Vec<ParamValue>, SurfaceError> {
        Ok(architect::lock(&self.values).iter().map(|(addr, value)| ParamValue { addr: addr.clone(), value: *value }).collect())
    }

    async fn set_param(&self, addr: ParamAddr, value: f64, _phase: EditPhase) -> Result<(), SurfaceError> {
        let Some(info) = self.catalog.param(&addr) else {
            return Err(SurfaceError::UnknownParam(addr.0));
        };
        // Store what the engine would actually hold (stepped params snap).
        let value = info.range.to_normalized(info.range.to_real(value));
        architect::lock(&self.values).insert(addr.clone(), value);
        self.changes.publish(ParamValue { addr, value });
        Ok(())
    }

    async fn invoke(&self, action: EntryId) -> Result<(), SurfaceError> {
        if self.catalog.action(&action).is_none() {
            return Err(SurfaceError::UnknownAction(action.0));
        }
        architect::lock(&self.invoked).push(action);
        Ok(())
    }

    async fn select(&self, collection: EntryId, index: u32) -> Result<(), SurfaceError> {
        if self.catalog.collection(&collection).is_none() {
            return Err(SurfaceError::UnknownCollection(collection.0));
        }
        architect::lock(&self.selected).insert(collection, index);
        Ok(())
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
        architect::layers![SurfaceEngineService, SurfaceEngineStreamService]
    }
}

/// A small guitar rig: an amp, a drive, a delay, a reverb, stomp actions,
/// meters and a tuner — enough to design and test real surfaces against.
#[must_use]
pub fn demo_catalog() -> Catalog {
    let db = |min, max| Range::linear(min, max, Unit::Db);
    let pct = Range::linear(0.0, 100.0, Unit::Percent);
    let param = |addr: &str, name: &str, group: &str, range: Range, default: f64| {
        ParamInfo::new(addr, name, range).in_group(group).with_default_real(default)
    };
    Catalog {
        id: "demo.guitar".into(),
        version: 1,
        params: vec![
            param("amp/gain", "Gain", "Amp", Range::linear(0.0, 10.0, Unit::None), 5.0),
            param("amp/bass", "Bass", "Amp", Range::linear(0.0, 10.0, Unit::None), 5.0),
            param("amp/mid", "Mid", "Amp", Range::linear(0.0, 10.0, Unit::None), 5.0),
            param("amp/treble", "Treble", "Amp", Range::linear(0.0, 10.0, Unit::None), 5.0),
            param("amp/presence", "Presence", "Amp", Range::linear(0.0, 10.0, Unit::None), 5.0),
            param("amp/master", "Master", "Amp", db(-60.0, 6.0), -12.0),
            ParamInfo::new("amp/channel", "Channel", Range::default()).in_group("Amp").choice(&["Clean", "Crunch", "Lead"]),
            param("drive/amount", "Drive", "Drive", pct.clone(), 40.0),
            param("drive/tone", "Tone", "Drive", pct.clone(), 50.0),
            ParamInfo::new("drive/on", "Drive On", Range::default()).in_group("Drive").toggle().short("Drive"),
            param("delay/time", "Time", "Delay", Range::log(20.0, 2000.0, Unit::Milliseconds), 350.0),
            param("delay/feedback", "Feedback", "Delay", pct.clone(), 30.0),
            param("delay/mix", "Mix", "Delay", pct.clone(), 20.0),
            ParamInfo::new("delay/on", "Delay On", Range::default()).in_group("Delay").toggle().short("Delay"),
            param("reverb/decay", "Decay", "Reverb", Range::log(0.2, 10.0, Unit::Seconds), 2.5),
            param("reverb/tone", "Tone", "Reverb", Range::log(500.0, 12_000.0, Unit::Hz), 4000.0),
            param("reverb/mix", "Mix", "Reverb", pct, 25.0),
            ParamInfo::new("reverb/on", "Reverb On", Range::default()).in_group("Reverb").toggle().short("Reverb"),
            param("rig/volume", "Volume", "Output", db(-60.0, 6.0), 0.0),
            ParamInfo { bipolar: true, ..param("rig/pan", "Pan", "Output", Range::linear(-100.0, 100.0, Unit::Percent), 0.0) },
        ],
        actions: vec![
            ActionInfo { id: EntryId::new("tap_tempo"), name: "Tap Tempo".into(), group: Some("Global".into()) },
            ActionInfo { id: EntryId::new("next_song"), name: "Next Song".into(), group: Some("Setlist".into()) },
            ActionInfo { id: EntryId::new("prev_song"), name: "Previous Song".into(), group: Some("Setlist".into()) },
            ActionInfo { id: EntryId::new("tuner"), name: "Tuner".into(), group: Some("Global".into()) },
        ],
        streams: vec![
            StreamInfo { id: EntryId::new("out_level"), name: "Output".into(), kind: StreamKind::Level { channels: 2 }, rate_hz: 30.0 },
            StreamInfo { id: EntryId::new("in_level"), name: "Input".into(), kind: StreamKind::Level { channels: 1 }, rate_hz: 30.0 },
            StreamInfo { id: EntryId::new("tuner"), name: "Tuner".into(), kind: StreamKind::Tuner, rate_hz: 30.0 },
            StreamInfo { id: EntryId::new("tempo"), name: "Tempo".into(), kind: StreamKind::Scalar { range: Range::linear(40.0, 240.0, Unit::Bpm) }, rate_hz: 4.0 },
        ],
        collections: vec![
            CollectionInfo { id: EntryId::new("songs"), name: "Songs".into() },
            CollectionInfo { id: EntryId::new("presets"), name: "Presets".into() },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use architect::{LocalServer, Scope, Services as _};

    async fn scenario(engine: &impl SurfaceEngine) {
        let catalog = engine.catalog().await.unwrap();
        assert!(catalog.param(&ParamAddr::new("amp/gain")).is_some());
        engine.set_param(ParamAddr::new("amp/gain"), 0.8, EditPhase::Change).await.unwrap();
        let v = engine.values().await.unwrap();
        assert_eq!(v.iter().find(|p| p.addr.0 == "amp/gain").unwrap().value, 0.8);
        assert_eq!(
            engine.set_param(ParamAddr::new("amp/nope"), 0.1, EditPhase::Change).await.unwrap_err(),
            SurfaceError::UnknownParam("amp/nope".into())
        );
        engine.invoke(EntryId::new("tap_tempo")).await.unwrap();
        assert!(engine.invoke(EntryId::new("self_destruct")).await.is_err());
        engine.select(EntryId::new("songs"), 3).await.unwrap();
    }

    #[tokio::test]
    async fn in_process() {
        scenario(&Memory::demo()).await;
    }

    #[tokio::test]
    async fn over_vox() {
        let scope = Scope::new();
        let local = LocalServer::serve(Memory::demo().into_router(), scope.clone());
        let client: SurfaceEngineClient = local.establish().await.unwrap();
        scenario(&client).await;
        scope.close().await;
    }

    #[tokio::test]
    async fn stepped_params_snap() {
        let m = Memory::demo();
        m.set_param(ParamAddr::new("amp/channel"), 0.4, EditPhase::Change).await.unwrap();
        assert_eq!(m.value(&ParamAddr::new("amp/channel")), Some(0.5));
        m.set_param(ParamAddr::new("drive/on"), 0.7, EditPhase::Change).await.unwrap();
        assert_eq!(m.value(&ParamAddr::new("drive/on")), Some(1.0));
    }

    #[test]
    fn demo_catalog_is_consistent() {
        let c = demo_catalog();
        let mut addrs: Vec<_> = c.params.iter().map(|p| &p.addr).collect();
        let n = addrs.len();
        addrs.sort();
        addrs.dedup();
        assert_eq!(addrs.len(), n, "duplicate addresses");
        assert!(c.params.iter().all(|p| (0.0..=1.0).contains(&p.default)));
        assert_eq!(c.groups().first().map(|(g, _)| *g), Some(Some("Amp")));
    }
}
