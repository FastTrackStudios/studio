//! The async bridge between a [`Runtime`](crate::Runtime) and an engine.
//!
//! Works with anything implementing [`SurfaceEngine`] — the in-process
//! backend, or a generated vox/HTTP client — so the same surface runs
//! embedded in a plugin, in the desktop app, or remotely on an iPad.

use surface_proto::{SurfaceEngine, SurfaceError};

use crate::runtime::{EngineCall, Runtime, Surface};

/// Fetch the catalog and current values and build a runtime for `surface`.
///
/// # Errors
/// When the engine can't be reached or refuses.
pub async fn connect(engine: &impl SurfaceEngine, surface: Surface) -> Result<Runtime, SurfaceError> {
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
pub async fn send(engine: &impl SurfaceEngine, calls: Vec<EngineCall>) -> Result<(), SurfaceError> {
    for call in calls {
        match call {
            EngineCall::SetParam { addr, value, phase } => engine.set_param(addr, value, phase).await?,
            EngineCall::Invoke { action } => engine.invoke(action).await?,
            EngineCall::Select { collection, index } => engine.select(collection, index).await?,
        }
    }
    Ok(())
}
