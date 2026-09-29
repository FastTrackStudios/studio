//! The `surface` contract: how a control surface talks to an engine.
//!
//! - [`curve`]: the one response-curve type.
//! - [`param`]: what an engine exposes ([`Catalog`]: params, actions,
//!   streams, collections) and how values map to engineering units.
//! - [`control`]: what makes a design node a control ([`Control`]:
//!   kind, [`Binding`], MIDI mapping, slots) and validation against a catalog.
//! - [`SurfaceEngine`]: the service any engine implements (signal rigs, a
//!   plugin, the mock engine). Surfaces only ever talk to an engine through
//!   it — in-process, over a WebSocket, or from an iPad.

pub mod control;
pub mod curve;
pub mod param;

pub use control::*;
pub use curve::{Curve, CurvePoint};
pub use param::*;

/// One parameter's current value (normalized 0..1).
#[derive(Debug, Clone, PartialEq, facet::Facet, serde::Serialize, serde::Deserialize)]
pub struct ParamValue {
    pub addr: ParamAddr,
    pub value: f64,
}

/// One sample of a telemetry stream. `values` by stream kind:
/// `Level` — dBFS per channel; `GainReduction` — `[dB]`;
/// `Spectrum` — dB per bin; `Tuner` — `[midi_note, cents]`; `Scalar` — `[v]`.
#[derive(Debug, Clone, PartialEq, facet::Facet, serde::Serialize, serde::Deserialize)]
pub struct StreamFrame {
    pub id: EntryId,
    pub values: Vec<f64>,
}

/// Where a parameter edit is in its gesture, so engines can group undo and
/// automation writes (begin → changes → end).
#[derive(Debug, Clone, Copy, PartialEq, Eq, facet::Facet, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum EditPhase {
    Begin,
    Change,
    End,
}

#[architect::error]
#[derive(Eq)]
pub enum SurfaceError {
    #[error("no parameter `{0}`")]
    #[architect(http_status = 404)]
    UnknownParam(String),
    #[error("no action `{0}`")]
    #[architect(http_status = 404)]
    UnknownAction(String),
    #[error("no collection `{0}`")]
    #[architect(http_status = 404)]
    UnknownCollection(String),
    #[error("index {0} out of range")]
    #[architect(http_status = 400)]
    OutOfRange(u32),
}

/// An engine, as a surface sees it.
#[architect::service]
pub trait SurfaceEngine {
    /// What this engine exposes.
    async fn catalog(&self) -> Result<Catalog, SurfaceError>;
    /// Every parameter's current value.
    async fn values(&self) -> Result<Vec<ParamValue>, SurfaceError>;
    /// Set a parameter (normalized). `phase` brackets a gesture.
    async fn set_param(&self, addr: ParamAddr, value: f64, phase: EditPhase) -> Result<(), SurfaceError>;
    /// Fire an action.
    async fn invoke(&self, action: EntryId) -> Result<(), SurfaceError>;
    /// Select an item of a collection.
    async fn select(&self, collection: EntryId, index: u32) -> Result<(), SurfaceError>;
    /// Parameter changes from anywhere (other surfaces, MIDI, automation).
    #[subscribe]
    fn param_changes(&self) -> ParamValue;
    /// Telemetry frames (meters, spectrum, tuner) at each stream's rate.
    #[subscribe]
    fn telemetry(&self) -> StreamFrame;
}

pub use prelude::*;
