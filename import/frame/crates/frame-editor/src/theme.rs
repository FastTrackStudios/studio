//! Compile an edited figcraft document into a theme a host can publish.
//!
//! This uses the v1 scene conversion and recompiles the whole document. It
//! does not yet provide incremental lowering or full figcraft parity.

use std::sync::Arc;

use frame::theme::runtime::{Diagnostic, Schema, Theme};
use frame_doc::doc::Document;

/// A conversion or theme compilation failure. The caller retains its last
/// published theme when this occurs.
#[derive(Debug)]
pub enum CompileError {
    Document(frame_doc::Error),
    Theme(Vec<Diagnostic>),
}

/// Compile the current edited document against the host's schema and mode.
///
/// The result is independent of the editor session and can be handed to a
/// host only after compilation succeeds. This call performs a full conversion
/// and compile; run it away from rendering and audio threads.
///
/// # Errors
/// The document cannot be converted, or theme compilation reports errors.
pub fn compile(
    document: &Document,
    schema: &Schema,
    mode: Option<&str>,
) -> Result<Arc<Theme>, CompileError> {
    let scene = frame_doc::doc_to_scene(document).map_err(CompileError::Document)?;
    Theme::compile(Arc::new(scene), schema.clone(), mode)
        .map(Arc::new)
        .map_err(CompileError::Theme)
}
