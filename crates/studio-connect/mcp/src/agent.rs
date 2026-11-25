//! The agent's calls beside the person's edits.
//!
//! Every action call goes through Studio's `AGENT_RUN` (the engine's,
//! `studio_engine::agent`): its edits are noted as the agent's in the
//! song's journal, its answer says the song's `revision`, an
//! `if_revision` makes it run only if the song has not moved on since
//! (refused as `stale`, with what changed), and a `step` makes it part of
//! one undo step with every other call of the same script or batch.
//!
//! A Studio from before `AGENT_RUN` answers `no action AGENT_RUN`: the
//! bridge then runs actions directly, as it did, and refuses an
//! `if_revision` it cannot honour. An action the engine does not have (an
//! app action: `STUDIO_SESSION_OPEN`) is run directly too.

use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Map, Value, json};

use crate::tools::Ctx;
use crate::upstream::{self, Failure, failure};

/// What the bridge knows of Studio's agent door.
#[derive(Debug, Default)]
pub struct Agent {
    /// Studio answered `no action AGENT_RUN`.
    missing: AtomicBool,
}

impl Agent {
    /// Whether Studio has `AGENT_RUN`, as far as the bridge knows yet.
    #[must_use]
    pub fn has_agent_run(&self) -> bool {
        !self.missing.load(Ordering::SeqCst)
    }
}

/// The name an agent's undo step is given: `Agent: <name>`.
#[must_use]
pub fn step_name(name: Option<&str>, default: &str) -> String {
    let name = name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(default);
    if name.starts_with("Agent") {
        name.to_owned()
    } else {
        format!("Agent: {name}")
    }
}

fn is_missing(f: &Failure) -> bool {
    f.get("code").and_then(Value::as_str) == Some("unknown")
        && f.get("error")
            .and_then(Value::as_str)
            .is_some_and(|e| e.contains("AGENT_RUN"))
}

/// Run action `id` with `args` (its own JSON; `if_revision` in it is the
/// bridge's) for the agent: Studio's answer, the song's revision noted on
/// the job.
///
/// # Errors
/// Studio's failure; `stale` when the song moved on past `if_revision`;
/// `unsupported` for an `if_revision` a Studio without `AGENT_RUN` cannot
/// check.
pub fn run(ctx: &Ctx<'_>, id: &str, args: &Value) -> Result<Value, Failure> {
    let mut args = args.clone();
    let if_revision = args
        .as_object_mut()
        .and_then(|o| o.remove("if_revision"))
        .filter(|v| !v.is_null());
    if ctx.agent.has_agent_run() {
        let mut params = json!({"id": id, "args": args});
        if let Some(o) = params.as_object_mut() {
            if let Some(r) = &if_revision {
                o.insert("if_revision".into(), r.clone());
            }
            if let Some(step) = ctx.step {
                o.insert("step".into(), json!(step));
            }
        }
        match upstream::run(ctx.up, "AGENT_RUN", &params, &ctx.job.cancel) {
            Ok(envelope) => {
                if let Some(r) = envelope.get("revision").and_then(Value::as_u64) {
                    ctx.job.saw_revision(r);
                }
                match envelope.get("ok").and_then(Value::as_bool) {
                    Some(true) => {
                        return Ok(envelope.get("answer").cloned().unwrap_or(Value::Null));
                    }
                    Some(false)
                        if envelope.get("code").and_then(Value::as_str) != Some("unknown") =>
                    {
                        return Err(refused(envelope));
                    }
                    // Not an engine action: the app's own, run directly.
                    Some(false) => {}
                    None => return Ok(envelope),
                }
            }
            Err(f) if is_missing(&f) => ctx.agent.missing.store(true, Ordering::SeqCst),
            Err(f) => return Err(f),
        }
    }
    if if_revision.is_some() && !ctx.agent.has_agent_run() {
        return Err(failure(
            "unsupported",
            "this Studio has no AGENT_RUN, so it cannot check `if_revision`: nothing was run. Call without it, or once Studio is rebuilt and restarted",
        ));
    }
    upstream::run(ctx.up, id, &args, &ctx.job.cancel)
}

/// AGENT_RUN's refusal as a failure: `{code, error, …}` (`revision`,
/// `since`, `changes` for `stale`; `takes` for `bad_args`).
fn refused(envelope: Value) -> Failure {
    let mut o: Map<String, Value> = envelope.as_object().cloned().unwrap_or_default();
    o.remove("ok");
    Value::Object(o)
}

/// End the undo step `step` (every edit made under it one undo step): its
/// name as an undo says it, or `None` when it changed nothing or Studio
/// cannot group.
#[must_use]
pub fn end_step(ctx: &Ctx<'_>, step: &str) -> Option<String> {
    if !ctx.agent.has_agent_run() {
        return None;
    }
    let ended = upstream::run(
        ctx.up,
        "AGENT_STEP_END",
        &json!({"step": step}),
        &crate::upstream::Cancel::default(),
    )
    .ok()?;
    if let Some(r) = ended.get("revision").and_then(Value::as_u64) {
        ctx.job.saw_revision(r);
    }
    ended.get("undo").and_then(Value::as_str).map(str::to_owned)
}

/// `answer` with the song's `revision`: an object gets the key (an
/// action's own `revision`, SIGNAL_CHAIN's, moves to `action_revision`);
/// anything else is wrapped `{answer, revision}`.
#[must_use]
pub fn with_revision(answer: Value, revision: Option<u64>) -> Value {
    let Some(revision) = revision else {
        return answer;
    };
    match answer {
        Value::Object(mut o) => {
            if let Some(own) = o.remove("revision")
                && own != json!(revision)
            {
                o.insert("action_revision".into(), own);
            }
            o.insert("revision".into(), json!(revision));
            Value::Object(o)
        }
        other => json!({"answer": other, "revision": revision}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_is_named_for_the_agent() {
        assert_eq!(
            step_name(Some("tighten snare"), "script"),
            "Agent: tighten snare"
        );
        assert_eq!(step_name(None, "script"), "Agent: script");
        assert_eq!(step_name(Some("Agent: x"), "script"), "Agent: x");
    }

    #[test]
    fn every_answer_says_its_revision() {
        assert_eq!(
            with_revision(json!({"a": 1}), Some(4)),
            json!({"a": 1, "revision": 4})
        );
        assert_eq!(
            with_revision(json!({"revision": 9, "chain": {}}), Some(4)),
            json!({"action_revision": 9, "chain": {}, "revision": 4})
        );
        assert_eq!(
            with_revision(json!([1]), Some(4)),
            json!({"answer": [1], "revision": 4})
        );
        assert_eq!(with_revision(json!([1]), None), json!([1]));
    }
}
