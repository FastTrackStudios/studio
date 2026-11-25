//! Long calls as jobs.
//!
//! Every tool call runs on a thread of its own and is waited for a while
//! (`wait`, 20 s unless given). One that finishes by then is answered as
//! it is; one that does not (a whole-song `MIX_MEASURE` takes minutes)
//! answers its job id at once and goes on running: `job {job}` waits for
//! it again and gives its result, `jobs` lists them, `job_cancel` stops
//! waiting for it and drops its connection to Studio.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::upstream::{Cancel, Failure};

/// How many finished jobs are kept to be read.
const KEPT_DONE: usize = 32;

/// Where a job is.
#[derive(Clone, Debug)]
pub enum State {
    Running,
    Done(Result<Value, Failure>),
    Cancelled,
}

/// One call, running or done.
#[derive(Debug)]
pub struct Job {
    pub id: String,
    pub tool: String,
    started: Instant,
    state: Mutex<(State, Option<Duration>)>,
    changed: Condvar,
    /// What it is doing now: a composite tool's step (`2/2 LISTEN …`).
    step: Mutex<String>,
    pub cancel: Cancel,
    /// It outlived its wait and was answered as a job: `jobs` lists it.
    pub background: AtomicBool,
}

impl Job {
    /// Say what it is doing now.
    pub fn step(&self, what: impl Into<String>) {
        *self.step.lock().unwrap_or_else(PoisonError::into_inner) = what.into();
    }

    fn finish(&self, outcome: Result<Value, Failure>) {
        {
            let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            if matches!(s.0, State::Running) {
                *s = (State::Done(outcome), Some(self.started.elapsed()));
            }
        }
        self.changed.notify_all();
    }

    /// Stop it: its waiting ends now.
    pub fn cancel(&self) {
        self.cancel.cancel();
        {
            let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            if matches!(s.0, State::Running) {
                *s = (State::Cancelled, Some(self.started.elapsed()));
            }
        }
        self.changed.notify_all();
    }

    /// Its state, waiting up to `wait` for it to finish.
    pub fn wait(&self, wait: Duration) -> State {
        let deadline = Instant::now().checked_add(wait);
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        while matches!(s.0, State::Running) {
            let left = deadline.map_or(Duration::ZERO, |d| {
                d.saturating_duration_since(Instant::now())
            });
            if left.is_zero() {
                break;
            }
            s = self
                .changed
                .wait_timeout(s, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        s.0.clone()
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        matches!(
            self.state.lock().unwrap_or_else(PoisonError::into_inner).0,
            State::Running
        )
    }

    /// How it stands, without its result: what `jobs` lists and a running
    /// job's answer says.
    #[must_use]
    pub fn status(&self) -> Value {
        let (state, took) = {
            let s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            let state = match &s.0 {
                State::Running => "running",
                State::Done(Ok(_)) => "done",
                State::Done(Err(_)) => "failed",
                State::Cancelled => "cancelled",
            };
            (state, s.1.unwrap_or_else(|| self.started.elapsed()))
        };
        let step = self
            .step
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let mut out = json!({
            "job": self.id,
            "tool": self.tool,
            "state": state,
            "elapsed_s": (took.as_secs_f64() * 10.0).round() / 10.0,
        });
        if !step.is_empty()
            && let Some(o) = out.as_object_mut()
        {
            o.insert("step".into(), json!(step));
        }
        out
    }
}

/// Every job of this session.
#[derive(Debug, Default)]
pub struct Jobs {
    all: Mutex<(u64, BTreeMap<u64, Arc<Job>>)>,
}

impl Jobs {
    /// Start `work` as a job of `tool`.
    pub fn spawn<F>(&self, tool: &str, work: F) -> Arc<Job>
    where
        F: FnOnce(&Job) -> Result<Value, Failure> + Send + 'static,
    {
        let job = {
            let mut all = self.all.lock().unwrap_or_else(PoisonError::into_inner);
            all.0 = all.0.saturating_add(1);
            let n = all.0;
            let job = Arc::new(Job {
                id: format!("j{n}"),
                tool: tool.to_owned(),
                started: Instant::now(),
                state: Mutex::new((State::Running, None)),
                changed: Condvar::new(),
                step: Mutex::new(String::new()),
                cancel: Cancel::default(),
                background: AtomicBool::new(false),
            });
            all.1.insert(n, job.clone());
            // Forget the oldest finished ones past what is kept.
            let done: Vec<u64> = all
                .1
                .iter()
                .filter(|(_, j)| !j.is_running())
                .map(|(k, _)| *k)
                .collect();
            for k in done.iter().take(done.len().saturating_sub(KEPT_DONE)) {
                all.1.remove(k);
            }
            job
        };
        let running = job.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("studio-mcp {}", job.id))
            .spawn(move || {
                let outcome = work(&running);
                running.finish(outcome);
            });
        if let Err(e) = spawned {
            job.finish(Err(
                json!({"code": "failed", "error": format!("could not start: {e}")}),
            ));
        }
        job
    }

    /// Job `id`, or why not.
    ///
    /// # Errors
    ///
    /// No such job: the ones there are.
    pub fn get(&self, id: &str) -> Result<Arc<Job>, String> {
        let all = self.all.lock().unwrap_or_else(PoisonError::into_inner);
        all.1.values().find(|j| j.id == id).cloned().ok_or_else(|| {
            let ids: Vec<&str> = all.1.values().map(|j| j.id.as_str()).collect();
            if ids.is_empty() {
                format!("no job {id}: none has been started")
            } else {
                format!("no job {id}; there are: {}", ids.join(", "))
            }
        })
    }

    /// The status of every job answered as one (it outlived its wait),
    /// newest first.
    #[must_use]
    pub fn list(&self) -> Vec<Value> {
        let all = self.all.lock().unwrap_or_else(PoisonError::into_inner);
        all.1
            .values()
            .rev()
            .filter(|j| j.background.load(Ordering::SeqCst))
            .map(|j| j.status())
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_quick_job_is_done_within_its_wait() {
        let jobs = Jobs::default();
        let job = jobs.spawn("listen", |_| Ok(json!({"lufs": -20})));
        match job.wait(Duration::from_secs(5)) {
            State::Done(Ok(v)) => assert_eq!(v, json!({"lufs": -20})),
            other => panic!("{other:?}"),
        }
        assert_eq!(job.status()["state"], json!("done"));
    }

    #[test]
    fn a_slow_job_runs_on_and_is_read_when_it_ends() {
        let jobs = Jobs::default();
        let (go, gate) = std::sync::mpsc::channel::<()>();
        let job = jobs.spawn("mix_measure", move |j| {
            j.step("1/1 MIX_MEASURE");
            gate.recv().ok();
            Ok(json!("measured"))
        });
        assert!(matches!(
            job.wait(Duration::from_millis(50)),
            State::Running
        ));
        let status = jobs.get(&job.id).unwrap().status();
        assert_eq!(status["state"], json!("running"));
        assert_eq!(status["step"], json!("1/1 MIX_MEASURE"));
        job.background.store(true, Ordering::SeqCst);
        go.send(()).unwrap();
        assert!(matches!(
            job.wait(Duration::from_secs(5)),
            State::Done(Ok(_))
        ));
        assert_eq!(jobs.list()[0]["state"], json!("done"));
    }

    #[test]
    fn a_cancelled_job_stays_cancelled() {
        let jobs = Jobs::default();
        let (go, gate) = std::sync::mpsc::channel::<()>();
        let job = jobs.spawn("balance", move |j| {
            gate.recv().ok();
            assert!(j.cancel.is_cancelled());
            Ok(json!("too late"))
        });
        job.cancel();
        assert!(matches!(job.wait(Duration::ZERO), State::Cancelled));
        go.send(()).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert!(matches!(job.wait(Duration::ZERO), State::Cancelled));
    }

    #[test]
    fn an_unknown_job_names_the_ones_there_are() {
        let jobs = Jobs::default();
        let job = jobs.spawn("x", |_| Ok(Value::Null));
        let e = jobs.get("j99").unwrap_err();
        assert!(e.contains(&job.id), "{e}");
    }
}
