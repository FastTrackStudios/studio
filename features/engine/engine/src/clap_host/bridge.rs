//! `com.fasttrackstudio.host-actions/1` over Studio's Action List: what a
//! CLAP in Studio calls to list, describe and run Studio's actions, to
//! subscribe to the song's events and to load a file by reference.
//!
//! A plugin runs an action as the song's agent does (the app's own door,
//! invoked by an agent), on a worker thread with the engine's usual
//! locking — never on the audio thread, never on the calling thread (see
//! the module above: the caller holds its plugin's lock).

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use architect::action::{ActionCaller, ActionError, ActionInfo, Invocation, InvocationSource};
use daw::standalone::clap_ext::ExtError;
use daw::standalone::clap_ext::host::{EventSink, HostActions};
use serde_json::{Value, json};

use super::topics::{self, Topic};

/// How long a plugin's call waits for its action before answering
/// `{"job": <id>}`, when it calls from a plugin lock Studio holds (its
/// `on_main_thread`, a `run_action`): its track is bypassed meanwhile.
pub const SYNC_WAIT_IN_PLUGIN: Duration = Duration::from_millis(20);

/// The same, from any other thread (the plugin's GUI).
pub const SYNC_WAIT: Duration = Duration::from_secs(2);

/// How many finished jobs are kept for `job` events still to go out.
const JOBS_KEPT: usize = 64;

/// Studio's side of host-actions/1, one for every plugin the engine loads.
#[derive(Default)]
pub struct StudioActions {
    jobs: Arc<Jobs>,
}

/// Actions still running past their call, by job id.
#[derive(Default)]
struct Jobs {
    next: AtomicU64,
    /// Finished, each with the job id and its event.
    done: Mutex<Vec<(u64, String)>>,
}

impl Jobs {
    fn id(&self) -> u64 {
        self.next.fetch_add(1, Ordering::AcqRel).saturating_add(1)
    }
}

impl HostActions for StudioActions {
    fn list(&self) -> Result<String, ExtError> {
        let infos = crate::actions::list().infos();
        if infos.is_empty() {
            return Err(ExtError::Unsupported);
        }
        Ok(Value::Array(infos.iter().map(described).collect()).to_string())
    }

    fn describe(&self, id: &str) -> Result<String, ExtError> {
        crate::actions::list()
            .info(id)
            .map(|info| described(&info).to_string())
            .ok_or_else(|| ExtError::NotFound(id.to_owned()))
    }

    fn run(&self, id: &str, args_json: &str) -> Result<String, ExtError> {
        let wait = if super::in_plugin_call() {
            SYNC_WAIT_IN_PLUGIN
        } else {
            SYNC_WAIT
        };
        run_waiting(&self.jobs, id, args_json, wait)
    }

    fn subscribe(&self, topic: &str, sink: EventSink) -> Option<u64> {
        let topic = Topic::parse(topic)?;
        Some(topics::subscribe(topic, sink))
    }

    fn unsubscribe(&self, subscription: u64) {
        topics::unsubscribe(subscription);
    }

    fn load_file(&self, reference: &str, blake3: Option<&str>) -> Result<Vec<u8>, ExtError> {
        let path = resolve_file(reference, &rig_library_roots())?;
        let bytes = std::fs::read(&path)
            .map_err(|e| ExtError::Failed(format!("{}: {e}", path.display())))?;
        if let Some(want) = blake3.filter(|h| !h.is_empty()) {
            let got = blake3::hash(&bytes).to_hex();
            if !got.as_str().eq_ignore_ascii_case(want) {
                return Err(ExtError::Failed(format!(
                    "{reference}: its blake3 is {got}, not {want}"
                )));
            }
        }
        Ok(bytes)
    }
}

/// One action as host-actions/1 lists it: `id`, `description`,
/// `args_schema` (the JSON Schema of what it takes, when it takes
/// anything) and the rest of what the Action List says of it.
fn described(info: &ActionInfo) -> Value {
    let schema = |s: &Option<String>| {
        s.as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
    };
    let mut out = json!({
        "id": info.id,
        "name": info.display_name,
        "description": info.description,
        "category": info.category,
        "effect": match info.effect {
            architect::action::Effect::ReadOnly => "read_only",
            architect::action::Effect::Mutating => "mutating",
            architect::action::Effect::Destructive => "destructive",
        },
        "enabled": info.enabled,
    });
    if let Some(args) = schema(&info.params_schema) {
        out["args_schema"] = args;
    }
    if let Some(result) = schema(&info.result_schema) {
        out["result_schema"] = result;
    }
    if let Some(why) = &info.disabled_reason {
        out["disabled_reason"] = Value::String(why.clone());
    }
    out
}

/// Run `id` on a worker and wait up to `wait` for it: its result, or
/// `{"job": <id>}` while it still runs (its result then goes out as a
/// `job` event).
fn run_waiting(
    jobs: &Arc<Jobs>,
    id: &str,
    args_json: &str,
    wait: Duration,
) -> Result<String, ExtError> {
    let span = tracing::info_span!(
        "clap.host_action",
        clap.action = id,
        clap.outcome = tracing::field::Empty,
        clap.job = tracing::field::Empty,
        clap.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let (tx, rx) = std::sync::mpsc::channel();
    // Whether the caller stopped waiting: held by the worker while it
    // hands its result over, and by the caller while it gives up, so a
    // result is either taken by the call or sent as the job's event.
    let gone = Arc::new(Mutex::new(false));
    let (action, args) = (id.to_owned(), args_json.to_owned());
    let job = jobs.id();
    let (kept, worker_gone) = (Arc::clone(jobs), Arc::clone(&gone));
    let spawned = std::thread::Builder::new()
        .name("clap-action".into())
        .spawn(move || {
            let result = run_action(&action, &args);
            let gone = worker_gone.lock().unwrap_or_else(PoisonError::into_inner);
            if *gone {
                drop(gone);
                finish_job(&kept, job, &action, &result);
            } else {
                let _ = tx.send(result);
            }
        });
    if let Err(e) = spawned {
        span.record("clap.outcome", "no-worker");
        return Err(ExtError::Failed(format!("no thread to run {id} on: {e}")));
    }
    let answered = rx.recv_timeout(wait).ok().or_else(|| {
        let mut gone = gone.lock().unwrap_or_else(PoisonError::into_inner);
        let last = rx.try_recv().ok();
        *gone = last.is_none();
        last
    });
    let out = match answered {
        Some(result) => {
            span.record("clap.outcome", if result.is_ok() { "ok" } else { "failed" });
            result.map_err(ExtError::Failed)
        }
        None => {
            span.record("clap.outcome", "job");
            span.record("clap.job", job);
            Ok(json!({ "job": job }).to_string())
        }
    };
    span.record("clap.ms", began.elapsed().as_secs_f64() * 1e3);
    out
}

/// A job finished: its `job` event, for the `job` subscribers.
fn finish_job(jobs: &Jobs, job: u64, action: &str, result: &Result<String, String>) {
    let event = match result {
        Ok(out) => json!({
            "job": job,
            "action": action,
            "ok": true,
            "result": serde_json::from_str::<Value>(out).unwrap_or(Value::Null),
        }),
        Err(why) => json!({ "job": job, "action": action, "ok": false, "error": why }),
    };
    // Queued for its subscribers first: a job kept as done has its event
    // on its way.
    topics::job_finished(event.to_string());
    let mut done = jobs.done.lock().unwrap_or_else(PoisonError::into_inner);
    done.push((job, event.to_string()));
    let over = done.len().saturating_sub(JOBS_KEPT);
    done.drain(..over);
}

/// Run one action as the song's agent would: the app's door, invoked by an
/// agent, awaited on the engine's runtime when it is async.
fn run_action(id: &str, args_json: &str) -> Result<String, String> {
    let list = crate::actions::list();
    let caller = ActionCaller::app().with_invocation(Invocation::new(InvocationSource::Agent));
    let args = if args_json.trim().is_empty() {
        b"{}".as_slice()
    } else {
        args_json.as_bytes()
    };
    let result = if list.is_async(id) {
        let runtime = crate::open::engine_runtime().map_err(|e| e.to_string())?;
        runtime.block_on(list.run_json_async_with(&caller, id, args))
    } else {
        list.run_json_with(&caller, id, args)
    };
    result
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|e| said(&e))
}

/// An action's failure as a plugin reads it: the Action List's words, with
/// what it expected when the arguments were wrong.
fn said(error: &ActionError) -> String {
    match error {
        ActionError::BadArgs {
            expected: Some(schema),
            ..
        } => format!("{error} (it takes {schema})"),
        other => other.to_string(),
    }
}

/// Where `rig-library:` references are looked for: `SIGNAL_RIG_LIBRARY`
/// (what the rigs' builds read), then the library Drive keeps.
#[must_use]
pub fn rig_library_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = std::env::var_os("SIGNAL_RIG_LIBRARY")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .into_iter()
        .collect();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        roots.push(home.join("Drive/Studio/Signal/rig-library"));
    }
    roots
}

/// The file `reference` names: `rig-library:<path in the library>` in
/// the first of `roots` that has it, or a path (absolute, or `~/…`). A
/// library path never leaves its library (no `..`).
///
/// # Errors
/// It names nothing on disk, or climbs out of the library.
pub fn resolve_file(reference: &str, roots: &[PathBuf]) -> Result<PathBuf, ExtError> {
    if let Some(rest) = reference.strip_prefix("rig-library:") {
        let rel = Path::new(rest.trim_start_matches('/'));
        if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err(ExtError::Failed(format!(
                "{reference}: a library path stays inside the library"
            )));
        }
        return roots
            .iter()
            .map(|root| root.join(rel))
            .find(|p| p.is_file())
            .ok_or_else(|| ExtError::NotFound(reference.to_owned()));
    }
    let path = reference.strip_prefix("~/").map_or_else(
        || PathBuf::from(reference),
        |rest| {
            std::env::var_os("HOME")
                .map_or_else(|| PathBuf::from(reference), |h| PathBuf::from(h).join(rest))
        },
    );
    if path.is_absolute() && path.is_file() {
        Ok(path)
    } else {
        Err(ExtError::NotFound(reference.to_owned()))
    }
}

/// The finished jobs' events kept, newest last (for a test, and a
/// subscriber that came late).
#[must_use]
pub fn finished_jobs(actions: &StudioActions) -> HashMap<u64, String> {
    actions
        .jobs
        .done
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .cloned()
        .collect()
}
