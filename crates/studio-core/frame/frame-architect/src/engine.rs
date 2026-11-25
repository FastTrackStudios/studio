//! The engine service: Frame's [`SurfaceEngine`](frame::surface::proto::SurfaceEngine)
//! over vox.
//!
//! `#[architect::rpc]` turns [`SurfaceEngine`] into `SurfaceEngineClient` /
//! `Service` / `surface_engine_serve`, plus the stream sibling
//! (`SurfaceEngineStreamClient`, `StreamService`, the
//! `SurfaceEngineStreamSource` backend contract for `param_changes` and
//! `telemetry`). The trait keeps the core trait's name, so the wire names are
//! the ones engines have always served.
//!
//! An engine implements the core trait (`frame::surface::proto::SurfaceEngine`)
//! and, to push changes and feeds, `SurfaceEngineStreamSource`; this crate's
//! trait follows for every core engine.

use frame::surface::proto::{
    Catalog, EditPhase, EntryId, FeedInfo, FeedRequest, ParamAddr, ParamValue, StreamFrame,
    SurfaceError, TextValue,
};

/// An engine, as a surface sees it, as a vox service (see the module docs).
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
    /// Ask for feeds shaped by options (see the core trait).
    fn request_feeds(&self, requests: Vec<FeedRequest>) -> Result<Vec<FeedInfo>, SurfaceError>;
    /// Every text parameter's current value.
    fn texts(&self) -> Vec<TextValue>;
    /// Set a text parameter (a band's name).
    fn set_text(&self, addr: ParamAddr, text: String) -> Result<(), SurfaceError>;
    /// Parameter changes from anywhere (other surfaces, MIDI, automation).
    #[subscribe]
    fn param_changes(&self) -> ParamValue;
    /// Feed frames (meters, spectrum, traces, scopes, envelopes, tuner)
    /// at each feed's rate, shaped as last requested.
    #[subscribe]
    fn telemetry(&self) -> StreamFrame;
}

impl<E: frame::surface::proto::SurfaceEngine> SurfaceEngine for E {
    fn catalog(&self) -> Catalog {
        frame::surface::proto::SurfaceEngine::catalog(self)
    }
    fn values(&self) -> Vec<ParamValue> {
        frame::surface::proto::SurfaceEngine::values(self)
    }
    fn set_param(&self, addr: ParamAddr, value: f64, phase: EditPhase) -> Result<(), SurfaceError> {
        frame::surface::proto::SurfaceEngine::set_param(self, addr, value, phase)
    }
    fn invoke(&self, action: EntryId) -> Result<(), SurfaceError> {
        frame::surface::proto::SurfaceEngine::invoke(self, action)
    }
    fn select(&self, collection: EntryId, index: u32) -> Result<(), SurfaceError> {
        frame::surface::proto::SurfaceEngine::select(self, collection, index)
    }
    fn request_feeds(&self, requests: Vec<FeedRequest>) -> Result<Vec<FeedInfo>, SurfaceError> {
        frame::surface::proto::SurfaceEngine::request_feeds(self, requests)
    }
    fn texts(&self) -> Vec<TextValue> {
        frame::surface::proto::SurfaceEngine::texts(self)
    }
    fn set_text(&self, addr: ParamAddr, text: String) -> Result<(), SurfaceError> {
        frame::surface::proto::SurfaceEngine::set_text(self, addr, text)
    }
}
