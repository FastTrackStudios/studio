//! The async bridge between a [`Runtime`](crate::Runtime) and an engine.
//!
//! [`Engine`] is what the runtime needs from an engine. It's implemented for
//! [`Local`] (any in-process [`SurfaceEngine`] backend — the mock, or a rig
//! embedded in the same process) and for the vox [`SurfaceEngineClient`]
//! (an engine across a WebSocket / iroh link), so the same surface runs
//! embedded in a plugin, in the desktop app, or remotely on an iPad.

use surface_proto::engine::SurfaceEngine;
#[cfg(feature = "vox")]
use surface_proto::engine::SurfaceEngineClient;
use surface_proto::{Catalog, ParamValue};

use crate::runtime::{EngineCall, Runtime, Surface};

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
        };
        r.map_err(|e| EngineError(e.to_string()))
    }
}

#[cfg(feature = "vox")]
impl Engine for SurfaceEngineClient {
    async fn catalog(&self) -> Result<Catalog, EngineError> {
        SurfaceEngineClient::catalog(self).await.map_err(|e| EngineError(format!("{e:?}")))
    }

    async fn values(&self) -> Result<Vec<ParamValue>, EngineError> {
        SurfaceEngineClient::values(self).await.map_err(|e| EngineError(format!("{e:?}")))
    }

    async fn call(&self, call: EngineCall) -> Result<(), EngineError> {
        let r = match call {
            EngineCall::SetParam { addr, value, phase } => self.set_param(addr, value, phase).await,
            EngineCall::Invoke { action } => self.invoke(action).await,
            EngineCall::Select { collection, index } => self.select(collection, index).await,
        };
        r.map_err(|e| EngineError(format!("{e:?}")))
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
