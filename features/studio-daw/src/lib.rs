//! Studio's DAW view in Architect (`extension.toml`;
//! docs/design/studio-in-architect.md): a setlist or a song entered into
//! Session's shell, attached to the Studio engine.
//!
//! The extension in [`ext`], its view in [`view`], which notes become
//! which sessions in [`resolve`], and the engine side of the binary that
//! links it in in [`engine`].

pub mod engine;
pub mod ext;
pub mod resolve;
pub mod view;
