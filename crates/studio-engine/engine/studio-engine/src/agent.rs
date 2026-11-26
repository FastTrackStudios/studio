//! An agent's edits beside the person's (`apps/studio/docs/mcp.md`): the
//! actions on the song's journal (`AGENT_RUN`, `AGENT_STEP_BEGIN`,
//! `AGENT_STEP_END`, `SESSION_CHANGES`). The journal itself, and the
//! in-flight tracking every door into the Action List uses, is the core's
//! ([`engine_core::journal`]), re-exported here.

use std::collections::HashMap;
use std::sync::Mutex;

use architect::action::{ActionError, ActionList, ActionMeta, Effect};
use facet::Facet;
use serde_json::{Value, json};

pub use engine_core::journal::*;

// ── the actions ─────────────────────────────────────────────────────────

static RUN: ActionMeta = crate::actions::meta(
    "AGENT_RUN",
    "agent_run",
    "Run an action for an agent",
    "Run the action `id` with `args` (its own JSON) as an agent's call: its edits are noted as the agent's in the song's journal (SESSION_CHANGES), apart from the person's. `if_revision`: run it only if the song is still at that revision (what every answer of Studio's MCP says) — when it has moved on, nothing runs and the answer is `{ok: false, code: \"stale\", revision, since, changes}`, the edits since and who made them. `step`: the name of an undo step (`Agent: tighten snare`) this call's edits join, with every other call naming it until AGENT_STEP_END — SESSION_UNDO then takes them all back at once (a song open from its project file). Answers `{ok: true, answer, revision, edit, step}` (`edit`: whether this call moved the song on), or `{ok: false, code, error}` (`stale`, `bad_args` with `takes`, `unknown`, `failed`, `denied`, `disabled`, `confirm` with `ticket`).",
    "Agent",
    "",
);

static STEP_BEGIN: ActionMeta = crate::actions::meta(
    "AGENT_STEP_BEGIN",
    "agent_step_begin",
    "Begin an agent's undo step",
    "From now, every edit of the open song (an agent's, and the person's meanwhile) joins one undo step named `step` (`Agent: tighten snare`), until AGENT_STEP_END with the same name; SESSION_UNDO then takes it back whole, SESSION_REDO brings it back. Left open past `timeout_s` (3700 s), it is dropped and its edits stay steps of their own. A song open from its project file only: `{grouped: false, why}` otherwise. AGENT_RUN's `step` begins one by itself at its first edit.",
    "Agent",
    "",
);

static STEP_END: ActionMeta = crate::actions::meta(
    "AGENT_STEP_END",
    "agent_step_end",
    "End an agent's undo step",
    "End the undo step `step` (AGENT_STEP_BEGIN, or AGENT_RUN's `step`): `{undo, steps, revision}` — `undo` the step's name as SESSION_UNDO will say it (null when it changed nothing), `steps` how many edits it holds. Nothing open of that name answers `undo: null`.",
    "Agent",
    "",
);

static CHANGES: ActionMeta = crate::actions::meta(
    "SESSION_CHANGES",
    "session_changes",
    "What changed in the song",
    "The song's edits after revision `since` (every answer of Studio's MCP says the revision it read; 0 or none: everything kept), oldest first: each `{revision, by (person or agent), action, label, kind (track, fx, send, items, marker, mix, undo, song), track, block, fields, at}`. Paged by `offset`/`limit` (50, 500 at most). `revision` is the song's now; `complete: false` when older edits were forgotten (the last 2000 are kept) or `since` is from before the engine started. View changes (selection, zoom, transport) are not edits. Read-only.",
    "Agent",
    "",
);

/// What AGENT_RUN takes, for its schema (it reads the JSON itself: `args`
/// is any object).
#[derive(Facet)]
struct RunDoc {
    id: String,
    /// The action's own arguments, an object.
    args: Option<String>,
    if_revision: Option<u64>,
    step: Option<String>,
}

#[derive(Facet)]
struct StepDoc {
    step: String,
    timeout_s: Option<f64>,
}

#[derive(Facet)]
struct ChangesDoc {
    since: Option<u64>,
    offset: Option<u32>,
    limit: Option<u32>,
}

fn parse(id: &'static str, raw: &[u8], doc: &'static facet::Shape) -> Result<Value, ActionError> {
    serde_json::from_slice(if raw.is_empty() { b"{}" } else { raw }).map_err(|e| {
        ActionError::BadArgs {
            id: id.to_owned(),
            message: e.to_string(),
            expected: Some(architect::schema::json_schema(doc)),
        }
    })
}

fn bad(id: &'static str, message: &str, doc: &'static facet::Shape) -> ActionError {
    ActionError::BadArgs {
        id: id.to_owned(),
        message: message.to_owned(),
        expected: Some(architect::schema::json_schema(doc)),
    }
}

/// AGENT_RUN's failure, as the answer it gives.
fn refusal(id: &str, e: ActionError) -> Value {
    let (code, error, extra) = match e {
        ActionError::Unknown(id) => ("unknown", format!("no action {id}"), None),
        ActionError::BadArgs {
            id,
            message,
            expected,
        } => (
            "bad_args",
            format!("{id}: bad arguments: {message}"),
            expected
                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                .map(|t| ("takes", t)),
        ),
        ActionError::Failed { message, .. } => ("failed", format!("{id}: {message}"), None),
        ActionError::Denied { reason, .. } => {
            ("denied", format!("{id}: not allowed: {reason}"), None)
        }
        ActionError::Disabled { reason, .. } => {
            ("disabled", format!("{id}: disabled: {reason}"), None)
        }
        ActionError::ConfirmationRequired { ticket, .. } => (
            "confirm",
            format!("{id}: a person must approve this call first; retry with its ticket"),
            Some(("ticket", json!(ticket))),
        ),
        ActionError::Duplicate(id) => ("failed", format!("{id}: already registered"), None),
    };
    let mut v = json!({"ok": false, "code": code, "error": error, "revision": revision()});
    if let (Some((k, x)), Some(o)) = (extra, v.as_object_mut()) {
        o.insert(k.into(), x);
    }
    v
}

/// Undo steps an agent has open: by name, the song each was begun on.
fn steps_open() -> std::sync::MutexGuard<'static, HashMap<String, String>> {
    static OPEN: std::sync::LazyLock<Mutex<HashMap<String, String>>> =
        std::sync::LazyLock::new(Default::default);
    OPEN.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// How long an undo step may stay open before it is dropped.
const STEP_TIMEOUT_S: f64 = 3700.0;

/// Begin the undo step `step` on the open song (if it is not already):
/// whether it is one now, or why not.
fn step_begin(step: &str, timeout_s: f64) -> Result<String, String> {
    let song = crate::open::current_song().ok_or("no song open")?;
    let had = steps_open().get(step).cloned();
    if had.as_deref() == Some(song.as_str()) && crate::project_file::group_open(&song, step) {
        return Ok(song);
    }
    if !crate::project_file::group_begin(&song, step) {
        return Err("the song is not open from its project file, so its undo has no groups: each edit is its own undo step".to_owned());
    }
    steps_open().insert(step.to_owned(), song.clone());
    let (watched, name) = (song.clone(), step.to_owned());
    let timeout = std::time::Duration::from_secs_f64(timeout_s.clamp(1.0, 86_400.0));
    let _ = std::thread::Builder::new()
        .name("agent-step-timeout".into())
        .spawn(move || {
            std::thread::sleep(timeout);
            if crate::project_file::group_abandon(&watched, &name) {
                steps_open().remove(&name);
                tracing::warn!(undo.group = %name, "an agent's undo step was never ended: dropped, its edits steps of their own");
            }
        });
    Ok(song)
}

/// End the undo step `step`: its name, when it holds any edit, and how
/// many.
fn step_end(step: &str) -> (Option<String>, usize) {
    let Some(song) = steps_open().remove(step) else {
        return (None, 0);
    };
    match crate::project_file::group_end(&song, step) {
        Some(n) if n > 0 => (Some(step.to_owned()), n),
        _ => (None, 0),
    }
}

/// AGENT_RUN's work.
async fn agent_run(a: Value) -> Result<Vec<u8>, ActionError> {
    let id = a
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad(RUN.id, "`id` is missing", RunDoc::SHAPE))?
        .to_owned();
    let inner = match a.get("args") {
        None | Some(Value::Null) => json!({}),
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or_else(|_| json!(s)),
        Some(v) => v.clone(),
    };
    if let Some(want) = a.get("if_revision").filter(|v| !v.is_null()) {
        let want = want.as_u64().ok_or_else(|| {
            bad(
                RUN.id,
                "`if_revision` is a revision, a whole number",
                RunDoc::SHAPE,
            )
        })?;
        if let Some(stale) = check(want) {
            tracing::warn!(agent.action = %id, agent.since = want, agent.revision = revision(), "an agent's edit was refused: the song moved on");
            return Ok(stale.to_string().into_bytes());
        }
    }
    if crate::actions::list().info(&id).is_none() {
        return Ok(refusal(&id, ActionError::Unknown(id.clone()))
            .to_string()
            .into_bytes());
    }
    let step = a
        .get("step")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty());
    let grouped = match step {
        Some(step) if mutating(&id) => {
            let (step, timeout) = (step.to_owned(), STEP_TIMEOUT_S);
            let begun = crate::off_loop::blocking(move || step_begin(&step, timeout))
                .await
                .unwrap_or_else(Err);
            Some(begun.map(|_| ()))
        }
        _ => None,
    };
    let before = revision();
    let args = inner.to_string();
    let out = crate::off_loop::run_by(&id, args.as_bytes(), By::Agent).await;
    let now = revision();
    let answer = match out {
        Ok(bytes) => {
            let answer = serde_json::from_slice::<Value>(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
            let mut v =
                json!({"ok": true, "answer": answer, "revision": now, "edit": now != before});
            if let (Some(g), Some(o)) = (&grouped, v.as_object_mut()) {
                match g {
                    Ok(()) => o.insert("step".into(), json!(step)),
                    Err(why) => o.insert("step_why".into(), json!(why)),
                };
            }
            v
        }
        Err(e) => refusal(&id, e),
    };
    Ok(answer.to_string().into_bytes())
}

/// `AGENT_RUN`, `AGENT_STEP_BEGIN`, `AGENT_STEP_END` and `SESSION_CHANGES`.
///
/// # Errors
/// When any is already registered.
pub fn register(list: &ActionList) -> Result<(), ActionError> {
    list.register_dynamic_async(
        &RUN,
        Effect::Mutating,
        Some(RunDoc::SHAPE),
        None,
        std::sync::Arc::new(|raw: &[u8]| {
            let a = parse(RUN.id, raw, RunDoc::SHAPE)?;
            Ok(Box::pin(agent_run(a)) as architect::action::ActionFuture)
        }),
    )?;
    list.register_dynamic(
        &STEP_BEGIN,
        Effect::Mutating,
        Some(StepDoc::SHAPE),
        None,
        std::sync::Arc::new(|raw: &[u8]| {
            let a = parse(STEP_BEGIN.id, raw, StepDoc::SHAPE)?;
            let step = a
                .get("step")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| {
                    bad(
                        STEP_BEGIN.id,
                        "`step` is missing (its name)",
                        StepDoc::SHAPE,
                    )
                })?;
            let timeout = a
                .get("timeout_s")
                .and_then(Value::as_f64)
                .unwrap_or(STEP_TIMEOUT_S);
            let v = match step_begin(step, timeout) {
                Ok(_) => json!({"grouped": true, "step": step, "revision": revision()}),
                Err(why) => json!({"grouped": false, "why": why, "revision": revision()}),
            };
            Ok(v.to_string().into_bytes())
        }),
    )?;
    list.register_dynamic(
        &STEP_END,
        Effect::Mutating,
        Some(StepDoc::SHAPE),
        None,
        std::sync::Arc::new(|raw: &[u8]| {
            let a = parse(STEP_END.id, raw, StepDoc::SHAPE)?;
            let step = a
                .get("step")
                .and_then(Value::as_str)
                .ok_or_else(|| bad(STEP_END.id, "`step` is missing (its name)", StepDoc::SHAPE))?;
            let (undo, steps) = step_end(step);
            Ok(
                json!({"undo": undo, "steps": steps, "revision": revision()})
                    .to_string()
                    .into_bytes(),
            )
        }),
    )?;
    list.register_dynamic(
        &CHANGES,
        Effect::ReadOnly,
        Some(ChangesDoc::SHAPE),
        None,
        std::sync::Arc::new(|raw: &[u8]| {
            let a = parse(CHANGES.id, raw, ChangesDoc::SHAPE)?;
            let from = a.get("since").and_then(Value::as_u64).unwrap_or(0);
            let offset = a.get("offset").and_then(Value::as_u64).unwrap_or(0);
            let limit = a
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(50)
                .clamp(1, 500);
            Ok(journal().page(from, offset, limit).to_string().into_bytes())
        }),
    )?;
    Ok(())
}
