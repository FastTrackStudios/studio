//! Blocking work run off the async thread that asked for it.
//!
//! A vox connection runs its handlers inside its own driver loop, so a
//! handler that blocks holds up every other call on that connection until
//! it returns. [`blocking`] runs the work on the runtime's blocking pool
//! instead, so the connection keeps serving calls (studio-engine's
//! `off_loop::run` is the same door for an action).

/// Run blocking `work` on the runtime's blocking pool under the caller's
/// span and await it; outside a runtime it runs here. `Err` when its
/// thread ended without an answer (it panicked).
///
/// # Errors
/// Its thread panicked or was cancelled.
pub async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    if tokio::runtime::Handle::try_current().is_err() {
        return Ok(work());
    }
    let span = tracing::Span::current();
    tokio::task::spawn_blocking(move || span.in_scope(work))
        .await
        .map_err(|e| format!("its thread ended without an answer: {e}"))
}
