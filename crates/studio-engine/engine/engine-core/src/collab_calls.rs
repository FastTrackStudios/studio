//! What another domain asks of the live session (engine-collab's
//! `collab`), without depending on it: each call goes through the engine
//! around the core ([`crate::assembly`], or the body a window's modules
//! share), and does nothing with none installed.

use std::sync::Arc;

use engine_sync::SessionDoc;
use engine_sync::net::PresenceSink;

/// The song on screen's doc in the live session, its presence and who this
/// peer is in it: for the chart editor's carets and a chart rewritten in
/// place (`collab::chart_context`). `None` with no session.
#[must_use]
pub fn chart_context() -> Option<(SessionDoc, Arc<dyn PresenceSink>, String)> {
    crate::body::studio_body().chart_context()
}

/// The chart was edited here, on the song on screen
/// (`collab::local_chart`).
pub fn local_chart(text: &str) {
    if let Some(assembly) = crate::assembly::get() {
        (assembly.local_chart)(text);
    }
}
