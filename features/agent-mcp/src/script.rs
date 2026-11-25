//! `script`: JavaScript that drives Studio, many calls in one tool call.
//!
//! An agent's loop of measure → change → measure costs a round trip a
//! call; a script makes them all in one, with the loops, conditions and
//! arithmetic between them in code. The engine is `QuickJS` (`rquickjs`):
//! JSON is JavaScript's own data, so Studio's answers are plain objects,
//! and its promises are what lets independent renders run side by side.
//!
//! A script is the body of an async function. `studio.<tool>(args)` calls
//! a tool the way the tool itself does ([`tools::run_action`] and the
//! composites: the same compact answers), but whole — paging is only for
//! what goes back to the client, the script's own answer. Each call runs
//! on a thread of its own with its own connection to Studio (a child of
//! the script's job, so `job_cancel` stops them all), at most
//! [`IN_FLIGHT`] at once; the script's thread runs the JavaScript and
//! settles each call's promise as its answer comes back.
//!
//! The sandbox: `QuickJS` with its intrinsics only — no module loader, no
//! `std`/`os`, no timers — so a script reaches nothing but `studio`,
//! `log` and its `args`. Its memory is capped ([`MEMORY`]) and the whole
//! run is timed out (`timeout`), JavaScript and calls alike.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use rquickjs::context::EvalOptions;
use rquickjs::promise::PromiseState;
use rquickjs::{Context, Ctx as Js, Function, Object, Promise, Runtime};
use serde_json::{Value, json};

use crate::jobs::Job;
use crate::shape;
use crate::tools::{self, Ctx, Exec};
use crate::upstream::{Failure, failure};

/// The most memory a script's JavaScript may hold.
pub const MEMORY: usize = 64 << 20;
/// Its stack, below the job thread's own.
const STACK: usize = 1 << 20;
/// How many of its calls run at once; more wait their turn.
pub const IN_FLIGHT: usize = 8;
/// Seconds the whole run may take, unless `timeout` says.
pub const TIMEOUT_S: f64 = 300.0;
/// … and at most.
pub const MAX_TIMEOUT_S: f64 = 3600.0;
/// Log lines kept, and the characters of each.
const LOG_LINES: usize = 500;
const LOG_CHARS: usize = 2000;

/// The tools a script does not call as `studio.<tool>`: `run` is
/// `studio.run(id, args)` (Studio's own answer), `batch` is what a script
/// replaces, and a script does not start another.
const NOT_IN_SCRIPTS: &[&str] = &["run", "batch", "script"];

/// What an agent reads of `script`.
pub const DESCRIPTION: &str = "Run JavaScript (QuickJS) that drives Studio: many calls in one tool call — loops, conditions, measure → change → measure, renders side by side. `source` is the body of an async function: `await` the calls, `return` the answer (`args` holds the tool's `args`). \
`studio.<tool>(args)` is any tool above, by its name or camelCase (`studio.listen`, `studio.blockMeasure`, `studio.mixMeasure`, `studio.trackSet`, `studio.signalBlockSet`, `studio.mixSnapshot`, `studio.mixRestore`, `studio.sessionTree`, …), answering what the tool answers (compact; `detail` as the tool takes it), whole: no paging, no jobs. \
`studio.run(id, args)`: any action, Studio's own answer. `studio.snapshot(name)`: MIX_SNAPSHOT, a checkpoint to `mixRestore` to. \
`studio.all([promises or functions])`: side by side (8 calls at once at most) — start calls without `await` and await them together. \
A failed call throws an Error with `.code`, `.message`, `.failure` (Studio's) — catch it or the script fails with it. `log(...)` (or `console.log`) lines come back with the answer. \
Answers `{result, log, calls, ms, revision, undo}`; an uncaught error answers its message, `line`, `source` (that line) and the log so far. \
All the script's edits are ONE undo step in the person's session, `Agent: <name>` (`name`; answered as `undo`), so they undo it at once; what ran before a failure stays done. Answers carry the song's `revision`: pass it as `if_revision` to a change that relies on a read (`stale` thrown if the person moved the song on). Sandboxed: no files, network, processes or timers; 64 MB; `timeout` (300 s, 3600 at most) covers everything.";

/// The `script` tool's schema.
#[must_use]
pub fn schema() -> Value {
    json!({"type": "object", "properties": {
        "source": {"type": "string", "description": "The JavaScript: the body of an async function — `await` each studio call, `return` the answer."},
        "args": {"description": "Any JSON, read by the script as `args`."},
        "timeout": {"type": "number", "minimum": 1, "maximum": MAX_TIMEOUT_S, "description": "Seconds the whole script may take (300 unless given)."},
        "name": {"type": "string", "description": "What it does, naming the one undo step its edits make: `Agent: <name>` (`tighten snare`; `Agent: script` unless given)."},
    }, "required": ["source"]})
}

/// One call a script made, waiting for its turn.
struct Request {
    id: u32,
    name: String,
    args: String,
}

/// What a script logged.
#[derive(Default)]
struct Log {
    lines: Vec<String>,
    dropped: usize,
}

impl Log {
    fn push(&mut self, line: String) {
        if self.lines.len() >= LOG_LINES {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        let line = if line.chars().count() > LOG_CHARS {
            let mut cut: String = line.chars().take(LOG_CHARS).collect();
            cut.push('…');
            cut
        } else {
            line
        };
        self.lines.push(line);
    }

    fn value(&self) -> Value {
        let mut lines: Vec<Value> = self.lines.iter().map(|l| json!(l)).collect();
        if self.dropped > 0 {
            lines.push(json!(format!(
                "… {} more lines not kept (the first {LOG_LINES} are)",
                self.dropped
            )));
        }
        Value::Array(lines)
    }
}

/// `block_measure` → `blockMeasure`.
fn camel(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut up = false;
    for c in name.chars() {
        if c == '_' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// The names `studio.<tool>` has.
fn script_tools() -> Vec<&'static str> {
    tools::all()
        .iter()
        .map(|t| t.name)
        .filter(|n| !NOT_IN_SCRIPTS.contains(n))
        .collect()
}

/// What sets up `studio`, `log` and `args` before the script runs.
fn prelude(input: &Value) -> String {
    let names: Vec<Value> = script_tools()
        .into_iter()
        .map(|n| json!([n, camel(n)]))
        .collect();
    format!(
        r#""use strict";
(() => {{
  const send = globalThis.__send, line = globalThis.__line;
  delete globalThis.__send; delete globalThis.__line;
  const pending = new Map();
  let next = 0;
  const call = (name, label, args) => {{
    if (args !== undefined && (args === null || typeof args !== "object" || Array.isArray(args)))
      return Promise.reject(new TypeError(`studio.${{label}}: its arguments are one object`));
    const id = ++next;
    // Where in the script it was called: a failure's line.
    const at = new Error().stack;
    return new Promise((resolve, reject) => {{
      pending.set(id, [resolve, reject, label, at]);
      send(id, name, JSON.stringify(args ?? {{}}));
    }});
  }};
  globalThis.__settle = (id, ok, text) => {{
    const p = pending.get(id);
    if (!p) return;
    pending.delete(id);
    const v = JSON.parse(text);
    if (ok) {{ p[0](v); return; }}
    const e = new Error(typeof v.error === "string" ? v.error : JSON.stringify(v));
    e.name = "StudioError";
    e.code = v.code;
    e.call = p[2];
    e.failure = v;
    e.stack = p[3];
    p[1](e);
  }};
  const show = (x) => {{
    if (typeof x === "string") return x;
    if (x instanceof Error) return `${{x.name}}: ${{x.message}}`;
    try {{ const s = JSON.stringify(x); return s === undefined ? String(x) : s; }} catch {{ return String(x); }}
  }};
  const log = (...xs) => {{ line(xs.map(show).join(" ")); }};
  globalThis.log = log;
  globalThis.console = Object.freeze({{ log, info: log, warn: log, error: log, debug: log }});
  const studio = {{}};
  for (const [name, label] of {names}) {{
    studio[label] = (args) => call(name, label, args);
    if (label !== name) studio[name] = studio[label];
  }}
  studio.run = (id, args) => call("run", `run(${{JSON.stringify(id)}})`, {{ id, args: args ?? {{}} }});
  studio.snapshot = (name, opts) => call("mix_snapshot", "snapshot", {{ ...(opts ?? {{}}), name }});
  studio.all = (xs) => Promise.all(Array.from(xs, (x) => (typeof x === "function" ? x() : x)));
  globalThis.studio = Object.freeze(studio);
  globalThis.args = {input};
  globalThis.__finish = (v) => JSON.stringify(v === undefined ? null : v, (k, x) =>
    x instanceof Promise ? "[a Promise: await it]"
    : x instanceof Error ? {{ error: x.message, code: x.code }}
    : typeof x === "bigint" ? Number(x)
    : x === undefined ? null : x);
}})();
"#,
        names = Value::Array(names),
        input = input,
    )
}

/// The source as the async function it is the body of. The opening sits
/// on the script's first line, so lines number as the agent wrote them.
const OPEN: &str = "(async () => {";

fn wrapped(source: &str) -> String {
    format!("{OPEN}{source}\n}})()")
}

/// Run the `script` tool.
///
/// # Errors
///
/// The script's uncaught error (its line, the Studio failure it was),
/// its timeout, or bad arguments.
pub fn run(ctx: &Ctx<'_>, args: &Value) -> Result<Value, Failure> {
    let source = args
        .get("source")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| failure("bad_args", "`source` is missing (the script's JavaScript)"))?;
    let timeout = args
        .get("timeout")
        .and_then(Value::as_f64)
        .filter(|t| t.is_finite() && *t > 0.0)
        .unwrap_or(TIMEOUT_S)
        .min(MAX_TIMEOUT_S);
    let input = args.get("args").cloned().unwrap_or(Value::Null);
    // Every edit the script makes one undo step, named for it.
    let step = crate::agent::step_name(args.get("name").and_then(Value::as_str), "script");
    let ctx = &Ctx {
        step: Some(&step),
        ..*ctx
    };
    let started = Instant::now();
    let deadline = started
        .checked_add(Duration::from_secs_f64(timeout))
        .unwrap_or(started);
    let run = Run {
        ctx,
        source,
        timeout,
        started,
        deadline,
        stop: Arc::new(AtomicBool::new(false)),
    };
    let outcome = run.go(&input);
    let undo = crate::agent::end_step(ctx, &step);
    let with_undo = |mut v: Value| {
        if let Some(o) = v.as_object_mut() {
            o.insert("undo".into(), json!(undo));
        }
        v
    };
    outcome.map(with_undo).map_err(with_undo)
}

/// One script's run.
struct Run<'a, 'c> {
    ctx: &'a Ctx<'c>,
    source: &'a str,
    timeout: f64,
    started: Instant,
    deadline: Instant,
    /// Set when the run is over its time or cancelled: the JavaScript
    /// stops at its next instruction.
    stop: Arc<AtomicBool>,
}

/// How a run ended, before it is an answer.
enum End {
    Returned(Value),
    Threw(Failure),
}

impl Run<'_, '_> {
    fn ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn over(&self) -> bool {
        Instant::now() >= self.deadline
    }

    fn go(&self, input: &Value) -> Result<Value, Failure> {
        let internal = |e: rquickjs::Error| failure("failed", format!("the script engine: {e}"));
        let rt = Runtime::new().map_err(internal)?;
        rt.set_memory_limit(MEMORY);
        rt.set_max_stack_size(STACK);
        let stop = self.stop.clone();
        let deadline = self.deadline;
        rt.set_interrupt_handler(Some(Box::new(move || {
            stop.load(Ordering::SeqCst) || Instant::now() >= deadline
        })));
        let context = Context::full(&rt).map_err(internal)?;
        let log = Rc::new(RefCell::new(Log::default()));
        let mut calls = 0u64;
        let end = std::thread::scope(|scope| {
            context.with(|js| self.drive(&js, scope, input, &log, &mut calls))
        });
        let log = log.borrow().value();
        match end {
            End::Returned(result) => Ok(json!({
                "result": shape::round(&result, 4),
                "log": log,
                "calls": calls,
                "ms": self.ms(),
            })),
            End::Threw(mut f) => {
                if let Some(o) = f.as_object_mut() {
                    o.insert("log".into(), log);
                    o.insert("calls".into(), json!(calls));
                    o.insert("ms".into(), json!(self.ms()));
                }
                Err(f)
            }
        }
    }

    /// Evaluate the script and settle its calls until it is done.
    fn drive<'s, 'js>(
        &'s self,
        js: &Js<'js>,
        scope: &'s std::thread::Scope<'s, '_>,
        input: &Value,
        log: &Rc<RefCell<Log>>,
        calls: &mut u64,
    ) -> End {
        let queue: Rc<RefCell<VecDeque<Request>>> = Rc::default();
        if let Err(e) = Self::install(js, input, &queue, log) {
            return End::Threw(failure("failed", format!("the script engine: {e}")));
        }
        let mut options = EvalOptions::default();
        options.strict = false;
        options.filename = Some("script".into());
        let top: Promise<'js> = match js.eval_with_options(wrapped(self.source), options) {
            Ok(p) => p,
            Err(_) => return End::Threw(self.thrown(js, &js.catch())),
        };
        let (tx, rx) = mpsc::channel::<(u32, Result<Value, Failure>)>();
        let mut running: Vec<Arc<Job>> = Vec::new();
        let mut in_flight = 0usize;
        loop {
            while js.execute_pending_job() {}
            if self.stop.load(Ordering::SeqCst) || self.over() {
                break;
            }
            while in_flight < IN_FLIGHT {
                let Some(req) = queue.borrow_mut().pop_front() else {
                    break;
                };
                *calls = calls.saturating_add(1);
                in_flight = in_flight.saturating_add(1);
                let child = self.ctx.job.child(&req.name);
                running.push(child.clone());
                let (up, agent, step) = (self.ctx.up, self.ctx.agent, self.ctx.step);
                let tx = tx.clone();
                scope.spawn(move || {
                    let ctx = Ctx {
                        up,
                        job: child.as_ref(),
                        agent,
                        step,
                    };
                    let outcome = serde_json::from_str::<Value>(&req.args)
                        .map_err(|e| failure("bad_args", format!("not JSON: {e}")))
                        .and_then(|a| call(&ctx, &req.name, &a));
                    let _ = tx.send((req.id, outcome));
                });
            }
            self.ctx.job.step(format!(
                "script: {calls} calls, {in_flight} running, {:.0} s",
                self.started.elapsed().as_secs_f64()
            ));
            if in_flight == 0 && queue.borrow().is_empty() {
                break;
            }
            if self.ctx.job.cancel.is_cancelled() {
                self.stop.store(true, Ordering::SeqCst);
                break;
            }
            let left = self.deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left.min(Duration::from_millis(100))) {
                Ok((id, outcome)) => {
                    in_flight = in_flight.saturating_sub(1);
                    settle(js, id, outcome);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        // Over: what still runs is stopped (the scope joins it).
        let cancelled = self.ctx.job.cancel.is_cancelled();
        if self.stop.load(Ordering::SeqCst) || self.over() || in_flight > 0 {
            self.stop.store(true, Ordering::SeqCst);
            for j in &running {
                j.cancel();
            }
            return End::Threw(if cancelled {
                failure("cancelled", "the script's job was cancelled")
            } else {
                failure(
                    "timeout",
                    format!(
                        "the script ran past its timeout ({} s; `timeout` gives it more, {MAX_TIMEOUT_S} at most): its calls still running were stopped",
                        self.timeout
                    ),
                )
            });
        }
        self.outcome(js, &top)
    }

    /// What the script's promise came to.
    fn outcome<'js>(&self, js: &Js<'js>, top: &Promise<'js>) -> End {
        match top.state() {
            PromiseState::Resolved => {
                let text = match top.result::<rquickjs::Value<'js>>() {
                    Some(Ok(v)) => js
                        .globals()
                        .get::<_, Function>("__finish")
                        .and_then(|f| f.call::<_, String>((v,))),
                    _ => Err(rquickjs::Error::Exception),
                };
                text.map_or_else(
                    |_| End::Threw(self.thrown(js, &js.catch())),
                    |t| {
                        serde_json::from_str(&t).map_or_else(
                            |e| {
                                End::Threw(failure(
                                    "script",
                                    format!("its answer is not JSON: {e}"),
                                ))
                            },
                            End::Returned,
                        )
                    },
                )
            }
            PromiseState::Rejected => {
                let _ = top.result::<rquickjs::Value>();
                End::Threw(self.thrown(js, &js.catch()))
            }
            PromiseState::Pending => End::Threw(failure(
                "script",
                "the script is waiting for something that never comes: only studio calls settle (there are no timers)",
            )),
        }
    }

    fn install(
        js: &Js<'_>,
        input: &Value,
        queue: &Rc<RefCell<VecDeque<Request>>>,
        log: &Rc<RefCell<Log>>,
    ) -> rquickjs::Result<()> {
        let g = js.globals();
        let q = queue.clone();
        g.set(
            "__send",
            Function::new(js.clone(), move |id: u32, name: String, args: String| {
                q.borrow_mut().push_back(Request { id, name, args });
            })?,
        )?;
        let l = log.clone();
        g.set(
            "__line",
            Function::new(js.clone(), move |line: String| {
                l.borrow_mut().push(line);
            })?,
        )?;
        let mut options = EvalOptions::default();
        options.filename = Some("prelude".into());
        js.eval_with_options::<(), _>(prelude(input), options)
    }

    /// What a thrown value says: its message, the script line it came
    /// from and that line's text, and for a failed studio call Studio's
    /// own failure (its `code` the answer's).
    fn thrown<'js>(&self, js: &Js<'js>, v: &rquickjs::Value<'js>) -> Failure {
        if self.over() {
            return failure(
                "timeout",
                format!(
                    "the script ran past its timeout ({} s; `timeout` gives it more)",
                    self.timeout
                ),
            );
        }
        if self.stop.load(Ordering::SeqCst) {
            return failure("cancelled", "the script's job was cancelled");
        }
        let Some(o) = v.as_object() else {
            let text = js
                .json_stringify(v.clone())
                .ok()
                .flatten()
                .and_then(|s| s.to_string().ok())
                .unwrap_or_else(|| "undefined".to_owned());
            return failure("script", format!("the script threw {text}"));
        };
        let get = |o: &Object<'js>, k: &str| -> Option<String> {
            o.get::<_, Option<rquickjs::Coerced<String>>>(k)
                .ok()
                .flatten()
                .map(|c| c.0)
        };
        let message = get(o, "message").unwrap_or_default();
        let name = get(o, "name").unwrap_or_else(|| "Error".to_owned());
        let stack = get(o, "stack").unwrap_or_default();
        let studio: Option<Value> = o
            .get::<_, Option<rquickjs::Value>>("failure")
            .ok()
            .flatten()
            .and_then(|f| js.json_stringify(f).ok().flatten())
            .and_then(|s| s.to_string().ok())
            .and_then(|s| serde_json::from_str(&s).ok());
        let memory = message.contains("out of memory");
        let mut f = match &studio {
            Some(s) => {
                let mut f = s.clone();
                if !f.is_object() {
                    f = failure("failed", message);
                }
                f
            }
            None if memory => failure(
                "script",
                format!(
                    "{name}: {message} (a script holds {} MB at most)",
                    MEMORY >> 20
                ),
            ),
            None => failure("script", format!("{name}: {message}")),
        };
        if let Some(fo) = f.as_object_mut() {
            if let Some((line, column)) = position(&stack) {
                fo.insert("line".into(), json!(line));
                if let Some(text) = self.source.lines().nth(line.saturating_sub(1)) {
                    fo.insert("source".into(), json!(text.trim()));
                }
                if line == 1 {
                    fo.insert("column".into(), json!(column.saturating_sub(OPEN.len())));
                } else {
                    fo.insert("column".into(), json!(column));
                }
            }
            if studio.is_some() {
                fo.insert(
                    "thrown_by".into(),
                    json!("a studio call the script did not catch"),
                );
            }
        }
        f
    }
}

/// Settle call `id`'s promise with its outcome.
fn settle(js: &Js<'_>, id: u32, outcome: Result<Value, Failure>) {
    let (ok, text) = match outcome {
        Ok(v) => (true, v.to_string()),
        Err(f) => (false, f.to_string()),
    };
    let settled = js
        .globals()
        .get::<_, Function>("__settle")
        .and_then(|f| f.call::<_, ()>((id, ok, text)));
    if settled.is_err() {
        let _ = js.catch();
    }
}

/// The script's line and column a stack points at first.
fn position(stack: &str) -> Option<(usize, usize)> {
    let at = stack.find("script:")?;
    let rest = stack.get(at.saturating_add("script:".len())..)?;
    let mut parts = rest.split(|c: char| !c.is_ascii_digit());
    let line = parts.next()?.parse().ok()?;
    let column = parts.next().and_then(|c| c.parse().ok()).unwrap_or(0);
    Some((line, column))
}

/// One studio call of a script: `run` is Studio's own answer; every other
/// name is that tool, as the tool answers.
fn call(ctx: &Ctx<'_>, name: &str, args: &Value) -> Result<Value, Failure> {
    let mut args = args.clone();
    if let Some(o) = args.as_object_mut() {
        o.remove("wait");
    }
    if name == "run" {
        let id = args
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| failure("bad_args", "studio.run(id, args): `id` is missing"))?;
        let inner = args.get("args").cloned().unwrap_or_else(|| json!({}));
        return ctx.run(id, &inner).map(|a| read_at(a, ctx));
    }
    let all = tools::all();
    let Some(tool) = all
        .iter()
        .find(|t| t.name == name && !NOT_IN_SCRIPTS.contains(&t.name))
    else {
        return Err(failure("unknown", format!("no studio.{name}")));
    };
    let answer = match tool.exec {
        Exec::Action(id) => tools::run_action(ctx, id, &args),
        Exec::Composite(f) => f(ctx, &args),
    }?;
    Ok(read_at(answer, ctx))
}

/// A call's answer with the song's `revision` it read, when it is an
/// object (a list stays a list, for the script to `.filter`).
fn read_at(answer: Value, ctx: &Ctx<'_>) -> Value {
    if answer.is_object() {
        crate::agent::with_revision(answer, ctx.job.revision())
    } else {
        answer
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_become_camel_case() {
        assert_eq!(camel("block_measure"), "blockMeasure");
        assert_eq!(camel("listen"), "listen");
        assert_eq!(camel("signal_block_set"), "signalBlockSet");
        let names = script_tools();
        assert!(names.contains(&"listen") && names.contains(&"mix_snapshot"));
        assert!(!names.contains(&"run") && !names.contains(&"script"));
    }

    #[test]
    fn a_stack_says_the_line() {
        assert_eq!(
            position("    at <anonymous> (script:3:9)\n    at <eval> (script:1:1)"),
            Some((3, 9))
        );
        assert_eq!(position("    at prelude:4:2\n"), None);
    }

    #[test]
    fn a_long_log_keeps_its_first_lines() {
        let mut log = Log::default();
        for i in 0..(LOG_LINES + 3) {
            log.push(format!("{i}"));
        }
        log.push("x".repeat(LOG_CHARS + 10));
        let v = log.value();
        assert_eq!(v.as_array().unwrap().len(), LOG_LINES + 1);
        assert!(v[LOG_LINES].as_str().unwrap().contains("4 more"));
    }
}
