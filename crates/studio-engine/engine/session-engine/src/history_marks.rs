//! Every action call its own step in the song's history, and a named
//! mark before one when asked.
//!
//! The engine finishes a step and the project file's recorder reads it
//! into the song's doc on a thread of its own (`project_file::record_steps`).
//! Steps that finished while it was busy were read together as one commit
//! named for them all — "Place a drum hit; Place a drum hit; …" — and the
//! doc's read takes in everything the engine holds, so even recorded apart
//! the first would carry the second's edit. Two things keep the calls
//! apart:
//!
//! - **A barrier** ([`settle`]): a mutating drum action first waits for the
//!   recorder to have read every step before it off the engine, so its own
//!   edit starts a commit of its own — never for their write into the doc,
//!   which runs on a thread of its own (seconds for a whole kit's edit);
//!   and an action waits for its own steps to be read before it returns
//!   (`edit_trace::run_edit`), so the next finds nothing to wait for;
//! - **runs** ([`gather`]): each action call has a run number (the
//!   outermost action on its thread, `crate::history::running_run`); the
//!   recorder only gathers steps of the same run into one commit.
//!
//! `mark` ([`begin`]): a call that names one sets a mark of that name on
//! the song first, a restore point `SESSION_HISTORY_RESTORE {mark}` goes
//! back to.

use std::sync::atomic::{AtomicU64, Ordering};

/// Run numbers, one per outermost action call.
static RUNS: AtomicU64 = AtomicU64::new(1);

/// A fresh run number.
pub fn next_run() -> u64 {
    RUNS.fetch_add(1, Ordering::Relaxed)
}

/// The longest a call waits for the steps before it to be recorded: a
/// whole song's read included.
pub const SETTLE: std::time::Duration = std::time::Duration::from_secs(3);

/// Wait until every step the engine finished has been read by the
/// recorder (at most `most`): what the engine does next is in none of
/// their commits. Their doc writes are not waited on. Whether it got
/// there; one warn line when it did not.
pub fn settle(most: std::time::Duration) -> bool {
    settle_on(crate::project_file::steps_pending, most)
}

/// [`settle`], with `pending` saying how many steps wait.
fn settle_on(pending: impl Fn() -> usize, most: std::time::Duration) -> bool {
    if pending() == 0 {
        return true;
    }
    let span = tracing::info_span!(
        "history.settle",
        history.pending = pending(),
        history.waited_ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let mut nap = std::time::Duration::from_micros(200);
    while pending() > 0 {
        if began.elapsed() >= most {
            span.record("history.waited_ms", began.elapsed().as_secs_f64() * 1e3);
            tracing::warn!(
                history.pending = pending(),
                "the steps before an action were not recorded in time: it may share their commit"
            );
            return false;
        }
        std::thread::sleep(nap);
        nap = (nap * 2).min(std::time::Duration::from_millis(5));
    }
    span.record("history.waited_ms", began.elapsed().as_secs_f64() * 1e3);
    true
}

/// What [`begin`] did before an action: the mark it set, if asked for one.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Begun {
    pub mark: Option<String>,
}

/// Before a mutating drum action `action` runs: the steps before it
/// recorded, so its edit is a step of its own; and, with `mark`, a mark of
/// that name set on the song as it is (a second of the same name gets
/// ` 2`, ` 3`, …). Traced as `history.begin`.
///
/// # Errors
/// A mark was asked for and could not be set (the song not open from its
/// project file): the action must not run.
pub fn begin(action: &str, mark: Option<&str>) -> Result<Begun, String> {
    let span = tracing::info_span!(
        "history.begin",
        history.action = action,
        history.mark = tracing::field::Empty,
    );
    let _in = span.enter();
    // An action another runs is a part of its caller's step: nothing to
    // wait for between them.
    if !crate::history::nested() {
        settle(SETTLE);
    }
    let Some(wanted) = mark.map(str::trim).filter(|m| !m.is_empty()) else {
        return Ok(Begun::default());
    };
    let (_, project) = crate::actions::song()?;
    let daw::service::ProjectContext::Project(guid) = project else {
        return Err("no song open".to_owned());
    };
    let name =
        crate::project_file::mark(&guid, action, |taken| unique(wanted, taken)).map_err(|why| {
            format!("{action} was not run: the mark {wanted:?} could not be set — {why}")
        })?;
    span.record("history.mark", name.as_str());
    Ok(Begun { mark: Some(name) })
}

/// `wanted`, or with ` 2`, ` 3`, … after it when a mark is already called
/// that.
#[must_use]
pub fn unique(wanted: &str, taken: &[String]) -> String {
    std::iter::once(wanted.to_owned())
        .chain((2_u32..).map(|n| format!("{wanted} {n}")))
        .find(|n| !taken.contains(n))
        .unwrap_or_else(|| wanted.to_owned())
}

/// One step as the engine finished it: its song, its label, the action
/// call that made it (none: no action, a drag in the window) and what the
/// recorder keeps with it.
pub type Finished<T> = (String, String, Option<u64>, T);

/// The steps `finished` (in the order they finished) gathered into the
/// commits they are recorded as: consecutive steps of the same song and
/// the same action call are one commit, named for them all; a step of
/// another call starts one of its own — two calls are never one step.
/// Each commit keeps the first of its steps' `T`.
pub fn gather<T>(finished: impl IntoIterator<Item = Finished<T>>) -> Vec<(String, Vec<String>, T)> {
    let mut out: Vec<(String, Vec<String>, Option<u64>, T)> = Vec::new();
    for (project, label, run, keep) in finished {
        match out.last_mut() {
            Some((p, labels, r, _)) if *p == project && *r == run => labels.push(label),
            _ => out.push((project, vec![label], run, keep)),
        }
    }
    out.into_iter()
        .map(|(project, labels, _, keep)| (project, labels, keep))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_calls_are_two_steps_however_close() {
        let steps = vec![
            (
                "song".to_owned(),
                "Place a drum hit".to_owned(),
                Some(7),
                'a',
            ),
            (
                "song".to_owned(),
                "Place a drum hit".to_owned(),
                Some(8),
                'b',
            ),
            (
                "song".to_owned(),
                "Place a drum hit".to_owned(),
                Some(9),
                'c',
            ),
        ];
        let commits = gather(steps);
        assert_eq!(commits.len(), 3, "{commits:?}");
        assert!(commits.iter().all(|(_, labels, _)| labels.len() == 1));
        assert_eq!(commits[1].2, 'b');
    }

    #[test]
    fn one_calls_steps_are_one_commit() {
        let steps = vec![
            ("song".to_owned(), "Cut".to_owned(), Some(3), 1),
            ("song".to_owned(), "Fade".to_owned(), Some(3), 2),
            ("other".to_owned(), "Cut".to_owned(), Some(3), 3),
            ("song".to_owned(), "Drag".to_owned(), None, 4),
            ("song".to_owned(), "Drag".to_owned(), None, 5),
        ];
        let commits = gather(steps);
        let shape: Vec<(&str, usize)> = commits
            .iter()
            .map(|(p, labels, _)| (p.as_str(), labels.len()))
            .collect();
        assert_eq!(shape, [("song", 2), ("other", 1), ("song", 2)]);
        assert_eq!(commits[0].1.join("; "), "Cut; Fade");
    }

    #[test]
    fn a_mark_of_a_taken_name_gets_a_number() {
        let taken = vec!["Before bar 40".to_owned(), "Before bar 40 2".to_owned()];
        assert_eq!(unique("Before bar 40", &taken), "Before bar 40 3");
        assert_eq!(unique("Fresh", &taken), "Fresh");
    }

    #[test]
    fn settling_waits_for_the_recorder_and_gives_up_in_time() {
        let left = std::sync::atomic::AtomicUsize::new(3);
        let pending = || {
            let n = left.load(Ordering::SeqCst);
            if n > 0 {
                left.store(n - 1, Ordering::SeqCst);
            }
            n
        };
        assert!(settle_on(pending, std::time::Duration::from_secs(1)));
        assert!(!settle_on(|| 1, std::time::Duration::from_millis(5)));
    }

    #[test]
    fn run_numbers_are_fresh() {
        let (a, b) = (next_run(), next_run());
        assert_ne!(a, b);
    }
}
