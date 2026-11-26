//! Where every millisecond of a drum edit goes.
//!
//! Each drum-edit action opens one span named after itself
//! ([`edit_span!`]: `QUANTIZE`, `HITS_PLACE`, `EDIT_REVERT`, …) under the
//! Action List's own `action` span, with the edit's counts declared on it
//! (`edit.hits`, `edit.moved`, `edit.slices`, `edit.crossfades`,
//! `edit.stretched`, `edit.items_written`, `edit.tracks`, …) and recorded
//! as the edit learns them. Its stages are child spans (`edit.read`,
//! `edit.plan`, `edit.place`, `edit.check`, `history.write`, …).
//!
//! The project lock: every look at the song's state and every batch of
//! item edits ([`crate::song::Song::read`], [`crate::song::Song::apply`])
//! adds the time it held the lock to this thread's tally ([`held`]); the
//! action's span says how much of it was its own (`edit.lock_ms`) when it
//! closes ([`Traced`]).
//!
//! The window-queued path (an edit made by the window that follows the
//! engine, waited for by ticket) is traced as `edit.window_wait`, with one
//! warn line when it waits long ([`landed`]).

use std::cell::Cell;

thread_local! {
    /// Microseconds this thread has held a song's lock, ever.
    static LOCK_US: Cell<u64> = const { Cell::new(0) };
}

/// Run `locked` (a look at the song's state or a batch of edits, under
/// its lock), adding how long it took to this thread's tally.
pub fn held<R>(locked: impl FnOnce() -> R) -> R {
    let began = web_time::Instant::now();
    let out = locked();
    let us = u64::try_from(began.elapsed().as_micros()).unwrap_or(u64::MAX);
    LOCK_US.with(|t| t.set(t.get().saturating_add(us)));
    out
}

/// This thread's lock tally, microseconds.
#[must_use]
pub fn lock_us() -> u64 {
    LOCK_US.with(Cell::get)
}

/// A drum-edit action's span, named after the action, with every count an
/// edit says declared on it (recorded later, as they are known).
#[doc(hidden)]
#[macro_export]
macro_rules! __edit_span {
    ($name:literal) => {
        tracing::info_span!(
            $name,
            edit.hits = tracing::field::Empty,
            edit.moved = tracing::field::Empty,
            edit.slices = tracing::field::Empty,
            edit.crossfades = tracing::field::Empty,
            edit.stretched = tracing::field::Empty,
            edit.items_written = tracing::field::Empty,
            edit.tracks = tracing::field::Empty,
            edit.held = tracing::field::Empty,
            edit.conflicts = tracing::field::Empty,
            edit.takes = tracing::field::Empty,
            edit.pieces = tracing::field::Empty,
            edit.spans = tracing::field::Empty,
            edit.dry_run = tracing::field::Empty,
            edit.mark = tracing::field::Empty,
            edit.lock_ms = tracing::field::Empty,
            edit.outcome = tracing::field::Empty,
        )
    };
}
pub use __edit_span as edit_span;

/// A stage of an edit, with the counts a stage may say declared on it.
#[doc(hidden)]
#[macro_export]
macro_rules! __stage_span {
    ($name:literal) => {
        tracing::info_span!(
            $name,
            edit.hits = tracing::field::Empty,
            edit.moved = tracing::field::Empty,
            edit.slices = tracing::field::Empty,
            edit.crossfades = tracing::field::Empty,
            edit.stretched = tracing::field::Empty,
            edit.items_written = tracing::field::Empty,
            edit.tracks = tracing::field::Empty,
            edit.lock_ms = tracing::field::Empty,
        )
    };
}
pub use __stage_span as stage_span;

/// An action's span, entered for as long as this is held: when it is let
/// go, the span says how long the action held the project lock.
#[must_use = "hold it while the action runs"]
pub struct Traced {
    span: tracing::span::EnteredSpan,
    lock_at: u64,
}

impl Traced {
    /// Enter `span` (made by [`edit_span!`]).
    pub fn enter(span: tracing::Span) -> Self {
        Self {
            span: span.entered(),
            lock_at: lock_us(),
        }
    }

    /// Record `value` as `field` on the action's span.
    pub fn record(&self, field: &'static str, value: impl tracing::Value) {
        self.span.record(field, value);
    }

    /// Record the counts `n` as `field`.
    pub fn count(&self, field: &'static str, n: usize) {
        self.span
            .record(field, u64::try_from(n).unwrap_or(u64::MAX));
    }

    /// Record how the call came out: `ok`, or the start of its error.
    pub fn outcome<T, E: std::fmt::Display>(&self, result: &Result<T, E>) {
        match result {
            Ok(_) => self.record("edit.outcome", "ok"),
            Err(why) => {
                let why = why.to_string();
                let short: String = why.chars().take(120).collect();
                self.record("edit.outcome", short.as_str());
            }
        }
    }
}

impl Drop for Traced {
    fn drop(&mut self) {
        let us = lock_us().saturating_sub(self.lock_at);
        self.span.record(
            "edit.lock_ms",
            num_traits::ToPrimitive::to_f64(&us).unwrap_or(0.0) / 1e3,
        );
    }
}

/// Run the mutating drum action `action` under `span` (made by
/// [`edit_span!`]): the steps before it recorded first and, with `mark`, a
/// mark of that name set (`history_marks::begin`); its outcome on the span.
///
/// # Errors
/// The mark could not be set, or `run` failed.
pub fn run_edit<T>(
    span: tracing::Span,
    (action, mark): (&str, Option<&str>),
    run: impl FnOnce(&Traced) -> Result<T, String>,
) -> Result<T, String> {
    let traced = Traced::enter(span);
    let out = crate::history_marks::begin(action, mark).and_then(|begun| {
        if let Some(name) = &begun.mark {
            traced.record("edit.mark", name.as_str());
        }
        run(&traced)
    });
    // The steps it made read before it returns: the next action finds
    // nothing of this one's to wait for (their doc write runs on).
    if !crate::history::nested() {
        crate::history_marks::settle(crate::history_marks::SETTLE);
    }
    traced.outcome(&out);
    out
}

/// Record `n` as `field` on the span the caller is in (a stage made by
/// [`stage_span!`] or an action's span): nothing when it has no such field.
pub fn count_here(field: &'static str, n: usize) {
    tracing::Span::current().record(field, u64::try_from(n).unwrap_or(u64::MAX));
}

/// How long a window-queued edit may wait before it is worth a warn line,
/// seconds: an edit the window makes lands within a few frames.
pub const WINDOW_SLOW: f64 = 1.0;

/// Wait until the window has made the edit `ticket` was queued with, at
/// most `most` — traced as `edit.window_wait` (`edit.waited_ms`,
/// `edit.landed`), one warn line when it waited past [`WINDOW_SLOW`] or
/// never landed. Whether it landed.
pub fn landed(ticket: u64, most: std::time::Duration) -> bool {
    let span = tracing::info_span!(
        "edit.window_wait",
        edit.ticket = ticket,
        edit.waited_ms = tracing::field::Empty,
        edit.landed = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let done = crate::engine::wait_done(ticket, most);
    let waited = began.elapsed().as_secs_f64();
    span.record("edit.waited_ms", waited * 1e3);
    span.record("edit.landed", done);
    if !done {
        tracing::warn!(
            edit.ticket = ticket,
            edit.waited_ms = waited * 1e3,
            "an edit queued for the window did not land in time"
        );
    } else if waited > WINDOW_SLOW {
        tracing::warn!(
            edit.ticket = ticket,
            edit.waited_ms = waited * 1e3,
            "an edit waited on the window"
        );
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lock_tally_adds_what_each_look_held() {
        let before = lock_us();
        let got = held(|| {
            std::thread::sleep(std::time::Duration::from_millis(2));
            7
        });
        assert_eq!(got, 7);
        assert!(lock_us() - before >= 2_000, "{}", lock_us() - before);
    }

    #[test]
    fn an_action_span_takes_its_counts() {
        let traced = Traced::enter(edit_span!("TEST_EDIT"));
        traced.count("edit.hits", 3);
        traced.outcome(&Ok::<(), String>(()));
        drop(traced);
    }
}
