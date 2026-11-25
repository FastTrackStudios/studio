//! Who else is in the session and what they are doing, and what this peer
//! is doing: the session primitive's store (`session_sync::presence::store`,
//! re-exported here), and this peer's caret in Studio's chart editor.

pub use fts_session::presence::store::*;

/// This peer's caret, as char offsets (anchor, head).
pub static CARET: std::sync::Mutex<Option<(usize, usize)>> = std::sync::Mutex::new(None);

/// This peer's caret as stable positions in the shared chart, for
/// presence — `None` outside a session or before the editor opened.
#[must_use]
#[cfg(feature = "native")]
pub fn local_caret() -> Option<(Vec<u8>, Vec<u8>)> {
    let (anchor, head) = (*CARET.lock().ok()?)?;
    let (doc, _, _) = crate::body::studio_body().chart_context()?;
    Some((doc.chart_cursor(anchor)?, doc.chart_cursor(head)?))
}

/// The chart editor's caret, as char offsets (anchor, head).
pub fn set_chart_caret(caret: Option<(usize, usize)>) {
    if let Ok(mut slot) = CARET.lock() {
        *slot = caret;
    }
}
