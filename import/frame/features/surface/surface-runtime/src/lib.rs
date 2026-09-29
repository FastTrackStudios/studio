//! Runs a control surface.
//!
//! - [`gesture`]: the interaction contract as pure state machines.
//! - [`visual`]: control state → ops on the design's slot nodes.
//! - [`meter`]: level ballistics and dB scales.
//! - [`coalesce`]: outgoing edit rate limiting that never loses the end.
//! - [`runtime`]: design + controls + live values, driven by the host.
//! - [`sync`]: the async bridge to any [`SurfaceEngine`](surface_proto::SurfaceEngine).

pub mod coalesce;
pub mod gesture;
pub mod meter;
pub mod runtime;
pub mod sync;
pub mod visual;

pub use gesture::{Bounds, Input, Key, Modifiers};
pub use runtime::{EngineCall, Runtime, Surface};
pub use visual::ControlState;
