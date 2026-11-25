//! Frame: the design and control-surface engine.
//!
//! The document, surface, motion and feed protocols are always here; the
//! runtime, rendering, themes, file IO, Figma import, design tools and the
//! live and embedded hosts are each behind a feature.
//!
//! This crate has no architect, vox or Dioxus dependency (CI checks it).
//! The architect integration — the `SurfaceEngine` vox service, the mock
//! engine, remote engines and surface libraries — is `frame-architect`.

pub mod design;
pub mod document;
#[cfg(feature = "embed")]
pub mod embed;
pub mod feed;
pub mod figma;
#[cfg(feature = "live")]
pub mod live;
pub mod motion;
#[cfg(feature = "render")]
pub mod render;
pub mod surface;
pub mod theme;
