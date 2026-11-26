//! Engine actions run off the async thread that asked for them.
//!
//! A vox connection runs its handlers inside its own driver loop (vox
//! 0.10's `Driver::run` polls them in a `FuturesUnordered` rather than
//! spawning them), so a handler that blocks holds up every other call on
//! that connection until it returns. Most engine actions are sync and
//! blocking: a read takes milliseconds, a render (`MIX_MEASURE`, `LISTEN`)
//! takes seconds to minutes. Run inline from a handler (`EngineView/run`,
//! the `daw` extension's `DAW_RUN`), one render stopped the window's view
//! calls and transport until it finished, and the window went gray.
//!
//! [`run`] is the door those handlers use instead: an async action is
//! awaited as before (it waits without blocking), and a sync one runs on
//! the runtime's blocking pool, so the connection keeps serving calls.
//! [`blocking`] does the same for any blocking work.

use architect::action::ActionError;

/// Run action `id` with `args` (JSON) and await it, never blocking the
/// thread that awaits: an async action is awaited in place, a sync one
/// runs on the blocking pool under the caller's span. Outside a tokio
/// runtime it runs as [`architect::action::ActionList::run_json_async`]
/// would.
///
/// # Errors
/// As `run_json_async`: no such action, bad arguments, a refusal, or a
/// panic.
pub async fn run(id: &str, args: &[u8]) -> Result<Vec<u8>, ActionError> {
    run_by(id, args, crate::journal::By::Person).await
}

/// [`run`], its edits noted as `by`'s in the song's journal
/// (`crate::journal`): the person's through the window and the app, the
/// agent's through `AGENT_RUN`.
///
/// # Errors
/// As [`run`].
pub async fn run_by(id: &str, args: &[u8], by: crate::journal::By) -> Result<Vec<u8>, ActionError> {
    let list = crate::actions::list();
    let watch = crate::journal::watch(id);
    if list.is_async(id) || tokio::runtime::Handle::try_current().is_err() {
        let flight = watch.as_ref().map(|_| crate::journal::InFlight::enter());
        let out = list.run_json_async(id, args).await;
        drop(flight);
        if let Some(w) = watch {
            w.finish(by, out.is_ok(), args);
        }
        return out;
    }
    let (owned, args) = (id.to_owned(), args.to_vec());
    blocking(move || {
        let _in = crate::journal::InAction::enter();
        let out = list.run_json(&owned, &args);
        if let Some(w) = watch {
            w.finish(by, out.is_ok(), &args);
        }
        out
    })
    .await
    .unwrap_or_else(|why| {
        Err(ActionError::Failed {
            id: id.to_owned(),
            message: why,
        })
    })
}

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
