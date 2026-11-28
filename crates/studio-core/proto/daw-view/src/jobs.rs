//! What is being done off the window's thread, and how far along: reading
//! waveforms, opening and saving the project file — anything the window
//! should not wait on but the person should be able to see.
//!
//! A piece of work holds a [`Job`] while it runs and moves it on as it
//! goes; dropping it ends it. The bottom bar shows what is running
//! ([`running`]). Work that can stop part way (an offline render) starts
//! with [`start_cancellable`] and looks at [`Job::cancelled`] as it goes;
//! [`cancel`] asks it to stop.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

struct Inner {
    id: u64,
    label: String,
    done: AtomicUsize,
    total: AtomicUsize,
    /// Set by [`cancel`]; `None` for work that cannot stop part way.
    cancel: Option<AtomicBool>,
}

/// One piece of work, running while held.
#[derive(Clone)]
pub struct Job(Arc<Inner>);

/// A job running now, as [`running`] reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Running {
    /// Its own, for as long as the process runs — what tells two jobs of
    /// the same label apart (a set's songs open four at once, each an
    /// "Opening", each reading its "Waveforms").
    pub id: u64,
    pub label: String,
    pub done: usize,
    /// Steps in all; 0 for a job that does not know.
    pub total: usize,
}

fn all() -> &'static Mutex<Vec<Weak<Inner>>> {
    static JOBS: Mutex<Vec<Weak<Inner>>> = Mutex::new(Vec::new());
    &JOBS
}

/// Where this process's job ids start: its pid, high. A window shows its
/// engine's jobs beside its own, and two processes' ids must not meet —
/// while one process (iOS, a test) lists each job once, by its one id.
fn base() -> u64 {
    // A page is one process with no pid to ask for.
    #[cfg(target_arch = "wasm32")]
    return 0;
    #[cfg(not(target_arch = "wasm32"))]
    return u64::from(std::process::id()) << 32;
}

/// Start a piece of work named `label`, `total` steps long (0 for one
/// that does not know).
#[must_use]
pub fn start(label: &str, total: usize) -> Job {
    begin(label, total, None)
}

/// [`start`], for work that stops part way when [`cancel`]led: it reads
/// [`Job::cancelled`] as it goes.
#[must_use]
pub fn start_cancellable(label: &str, total: usize) -> Job {
    begin(label, total, Some(AtomicBool::new(false)))
}

fn begin(label: &str, total: usize, cancel: Option<AtomicBool>) -> Job {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let inner = Arc::new(Inner {
        id: base() + NEXT.fetch_add(1, Ordering::Relaxed),
        label: label.to_owned(),
        done: AtomicUsize::new(0),
        total: AtomicUsize::new(total),
        cancel,
    });
    if let Ok(mut jobs) = all().lock() {
        jobs.retain(|j| j.strong_count() > 0);
        jobs.push(Arc::downgrade(&inner));
    }
    Job(inner)
}

impl Job {
    /// `n` more steps done.
    pub fn advance(&self, n: usize) {
        self.0.done.fetch_add(n, Ordering::Relaxed);
    }

    /// Its id, as [`running`] lists it and [`cancel`] takes it.
    #[must_use]
    pub fn id(&self) -> u64 {
        self.0.id
    }

    /// Whether it has been asked to stop ([`cancel`]): never for work
    /// started with [`start`].
    #[must_use]
    pub fn cancelled(&self) -> bool {
        self.0
            .cancel
            .as_ref()
            .is_some_and(|c| c.load(Ordering::Relaxed))
    }
}

/// The running job `id`, if there is one.
fn find(id: u64) -> Option<Arc<Inner>> {
    all()
        .lock()
        .ok()?
        .iter()
        .filter_map(Weak::upgrade)
        .find(|j| j.id == id)
}

/// Whether running job `id` can be [`cancel`]led.
#[must_use]
pub fn cancellable(id: u64) -> bool {
    find(id).is_some_and(|j| j.cancel.is_some())
}

/// What [`cancel`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cancelled {
    /// Asked to stop: it stops at its next step.
    Asked,
    /// It cannot stop part way.
    NotCancellable,
    /// No job of that id is running (it may have just finished).
    NotRunning,
}

/// Ask running job `id` to stop.
pub fn cancel(id: u64) -> Cancelled {
    match find(id) {
        None => Cancelled::NotRunning,
        Some(j) => match &j.cancel {
            None => Cancelled::NotCancellable,
            Some(c) => {
                c.store(true, Ordering::Relaxed);
                Cancelled::Asked
            }
        },
    }
}

/// What is running now, in the order it started.
#[must_use]
pub fn running() -> Vec<Running> {
    all().lock().map_or_else(
        |_| Vec::new(),
        |jobs| {
            jobs.iter()
                .filter_map(Weak::upgrade)
                .map(|j| Running {
                    id: j.id,
                    label: j.label.clone(),
                    done: j.done.load(Ordering::Relaxed),
                    total: j.total.load(Ordering::Relaxed),
                })
                .collect()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{Cancelled, cancel, cancellable, running, start, start_cancellable};

    /// A render the person stops: asked once, it reads cancelled; work
    /// that cannot stop part way, or is gone, says so (studio#685).
    #[test]
    fn a_cancellable_job_is_asked_to_stop_and_others_say_why_not() {
        let render = start_cancellable("Measuring the mix", 10);
        let saving = start("Saving", 0);
        assert!(cancellable(render.id()) && !cancellable(saving.id()));
        assert!(!render.cancelled());
        assert_eq!(cancel(render.id()), Cancelled::Asked);
        assert!(render.cancelled());
        assert_eq!(cancel(saving.id()), Cancelled::NotCancellable);
        assert!(!saving.cancelled());
        let gone = render.id();
        drop(render);
        assert_eq!(cancel(gone), Cancelled::NotRunning);
    }

    /// The bottom bar keys a job's row by its id: songs of a set open in
    /// parallel, each its own "Opening" — keyed by label, four of them
    /// were four siblings with one key, and the window panicked.
    #[test]
    fn jobs_of_one_label_are_told_apart() {
        let jobs: Vec<_> = (0..4).map(|_| start("Opening", 0)).collect();
        let ids: std::collections::HashSet<u64> = running()
            .into_iter()
            .filter(|j| j.label == "Opening")
            .map(|j| j.id)
            .collect();
        assert_eq!(ids.len(), jobs.len());
        drop(jobs);
        assert!(running().iter().all(|j| j.label != "Opening"));
    }
}
