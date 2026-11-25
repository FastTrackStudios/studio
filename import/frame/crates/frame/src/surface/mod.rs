//! Surfaces: control surfaces — their protocol, runtime, widgets, kit and a
//! demo catalog. The in-memory mock engine is `frame-architect`'s.

pub mod demo;
pub mod proto;

#[cfg(feature = "runtime")]
pub mod runtime;

#[cfg(feature = "runtime")]
pub mod widgets;

#[cfg(feature = "runtime")]
pub mod kit;
