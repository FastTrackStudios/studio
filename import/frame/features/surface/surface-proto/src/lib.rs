//! The `surface` contract: how a control surface talks to an engine.
//!
//! - [`curve`]: the one response-curve type.
//! - [`param`]: what an engine exposes ([`Catalog`]: params, actions,
//!   streams, collections) and how values map to engineering units.
//! - [`control`]: what makes a design node a control ([`Control`]:
//!   kind, [`Binding`], MIDI mapping, slots) and validation against a catalog.
//! - [`engine::SurfaceEngine`]: the service any engine implements (signal rigs, a
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

/// Why an engine refused a call.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum SurfaceError {
    UnknownParam(String),
    UnknownAction(String),
    UnknownCollection(String),
    OutOfRange(u32),
}

impl std::fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownParam(a) => write!(f, "no parameter `{a}`"),
            Self::UnknownAction(a) => write!(f, "no action `{a}`"),
            Self::UnknownCollection(c) => write!(f, "no collection `{c}`"),
            Self::OutOfRange(i) => write!(f, "index {i} out of range"),
        }
    }
}

impl std::error::Error for SurfaceError {}

pub mod engine {
    //! The engine service. `SurfaceEngine` → `SurfaceEngineClient` /
    //! `SurfaceEngineService` / `surface_engine_serve`, plus the stream
    //! sibling (`SurfaceEngineStreamClient`, `SurfaceEngineStreamService`,
    //! the `SurfaceEngineStreamSource` backend contract).
    use super::{Catalog, EditPhase, EntryId, ParamAddr, ParamValue, StreamFrame, SurfaceError};

    /// An engine, as a surface sees it. Backends implement it synchronously
    /// (like signal's rig services); remote callers get the async client.
    #[architect::rpc]
    pub trait SurfaceEngine {
        /// What this engine exposes.
        fn catalog(&self) -> Catalog;
        /// Every parameter's current value.
        fn values(&self) -> Vec<ParamValue>;
        /// Set a parameter (normalized). `phase` brackets a gesture.
        fn set_param(&self, addr: ParamAddr, value: f64, phase: EditPhase) -> Result<(), SurfaceError>;
        /// Fire an action.
        fn invoke(&self, action: EntryId) -> Result<(), SurfaceError>;
        /// Select an item of a collection.
        fn select(&self, collection: EntryId, index: u32) -> Result<(), SurfaceError>;
        /// Parameter changes from anywhere (other surfaces, MIDI, automation).
        #[subscribe]
        fn param_changes(&self) -> ParamValue;
        /// Telemetry frames (meters, spectrum, tuner) at each stream's rate.
        #[subscribe]
        fn telemetry(&self) -> StreamFrame;
    }
}

pub use engine::*;
