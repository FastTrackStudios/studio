//! The async bridge between a [`Runtime`](crate::surface::runtime::Runtime) and an engine.
//!
//! [`Engine`] is what the runtime needs from an engine. It's implemented here
//! for [`Local`] (any in-process [`SurfaceEngine`] backend — a rig embedded
//! in the same process), and in `frame-architect` for the vox
//! `SurfaceEngineClient` (an engine across a WebSocket / iroh link), so the
//! same surface runs embedded in a plugin, in the desktop app, or remotely on
//! an iPad.

use crate::surface::proto::engine::SurfaceEngine;
use crate::surface::proto::{Catalog, FeedInfo, FeedRequest, ParamValue, TextValue};

use crate::surface::runtime::runtime::{EngineCall, Runtime, Surface};

/// Why talking to an engine failed (refused, or the link dropped).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineError(pub String);

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for EngineError {}

/// An engine as the runtime drives it.
#[allow(async_fn_in_trait)]
pub trait Engine {
    async fn catalog(&self) -> Result<Catalog, EngineError>;
    async fn values(&self) -> Result<Vec<ParamValue>, EngineError>;
    async fn call(&self, call: EngineCall) -> Result<(), EngineError>;
    /// Ask for feeds shaped for the surface (a plot's bins, a history's
    /// length). An engine that can't reshape keeps its catalog's shapes.
    async fn request_feeds(
        &self,
        requests: Vec<FeedRequest>,
    ) -> Result<Vec<FeedInfo>, EngineError> {
        let _ = requests;
        Ok(Vec::new())
    }
    /// Text params' values (names). An engine without text keeps none.
    async fn texts(&self) -> Result<Vec<TextValue>, EngineError> {
        Ok(Vec::new())
    }
}

/// An in-process backend.
pub struct Local<'a, E>(pub &'a E);

impl<E: SurfaceEngine> Engine for Local<'_, E> {
    async fn catalog(&self) -> Result<Catalog, EngineError> {
        Ok(self.0.catalog())
    }

    async fn values(&self) -> Result<Vec<ParamValue>, EngineError> {
        Ok(self.0.values())
    }

    async fn call(&self, call: EngineCall) -> Result<(), EngineError> {
        let r = match call {
            EngineCall::SetParam { addr, value, phase } => self.0.set_param(addr, value, phase),
            EngineCall::Invoke { action } => self.0.invoke(action),
            EngineCall::Select { collection, index } => self.0.select(collection, index),
            EngineCall::SetText { addr, text } => self.0.set_text(addr, text),
        };
        r.map_err(|e| EngineError(e.to_string()))
    }

    async fn request_feeds(
        &self,
        requests: Vec<FeedRequest>,
    ) -> Result<Vec<FeedInfo>, EngineError> {
        self.0
            .request_feeds(requests)
            .map_err(|e| EngineError(e.to_string()))
    }

    async fn texts(&self) -> Result<Vec<TextValue>, EngineError> {
        Ok(self.0.texts())
    }
}

/// Fetch the catalog and current values and build a runtime for `surface`.
///
/// # Errors
/// When the engine can't be reached.
pub async fn connect(engine: &impl Engine, surface: Surface) -> Result<Runtime, EngineError> {
    let catalog = engine.catalog().await?;
    let values = engine.values().await?;
    let mut rt = Runtime::new(surface, catalog);
    rt.apply_values(values);
    // Best effort: an engine that predates text params has none.
    if let Ok(texts) = engine.texts().await {
        rt.apply_texts(texts);
    }
    // Best effort: an engine that refuses keeps its catalog's shapes, and
    // plots draw whatever arrives.
    let requests = rt.feed_requests();
    if !requests.is_empty() {
        let _ = engine.request_feeds(requests).await;
    }
    Ok(rt)
}

/// Send calls in order. Stops at the first error.
///
/// # Errors
/// The first failing call's error.
pub async fn send(engine: &impl Engine, calls: Vec<EngineCall>) -> Result<(), EngineError> {
    for call in calls {
        engine.call(call).await?;
    }
    Ok(())
}
