//! The MCP side: JSON-RPC over stdio, one message a line.

use std::collections::BTreeMap;
use std::io::{BufRead as _, Write as _};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::{Value, json};

use crate::jobs::{Job, Jobs, State};
use crate::page::Pages;
use crate::tools::{self, Ctx, Exec, Tool};
use crate::upstream::{Cancel, Failure, Shared, failure};

/// The protocol revisions it answers in.
const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// How the bridge behaves.
#[derive(Clone, Debug)]
pub struct Config {
    /// How long a call is waited for before it is answered as a job.
    pub wait: Duration,
    /// The most JSON one answer carries before it pages.
    pub budget: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            wait: Duration::from_secs(20),
            budget: 24_000,
        }
    }
}

/// Where notifications (progress, the tool list changed) go.
pub type Notify = Arc<dyn Fn(Value) + Send + Sync>;

/// One agent's session with Studio.
pub struct Bridge {
    up: Shared,
    config: Config,
    jobs: Jobs,
    pages: Pages,
    notify: Notify,
    /// Each action tool's schema and effect, as Studio last described it.
    schemas: Mutex<BTreeMap<&'static str, (Value, String)>>,
    /// A tool list was given while Studio did not answer: say when it does.
    listed_without: AtomicBool,
    /// What it knows of Studio's agent door (`AGENT_RUN`).
    agent: Arc<crate::agent::Agent>,
}

impl Bridge {
    #[must_use]
    pub fn new(up: Shared, config: Config, notify: Notify) -> Self {
        Self {
            up,
            config,
            jobs: Jobs::default(),
            pages: Pages::default(),
            notify,
            schemas: Mutex::new(BTreeMap::new()),
            listed_without: AtomicBool::new(false),
            agent: Arc::default(),
        }
    }

    /// One JSON-RPC message in; its reply, if it wants one.
    pub fn handle(&self, msg: &Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let method = msg
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
        let id = id?;
        let result = match method {
            "initialize" => Ok(Self::initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": self.list() })),
            "tools/call" => self.call(&params),
            "resources/list" => Ok(json!({ "resources": [] })),
            "prompts/list" => Ok(json!({ "prompts": [] })),
            other => Err((-32_601, format!("no method {other}"))),
        };
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err((code, message)) => {
                json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
            }
        })
    }

    fn initialize(params: &Value) -> Value {
        let asked = params.get("protocolVersion").and_then(Value::as_str);
        let version = asked
            .filter(|v| PROTOCOL_VERSIONS.contains(v))
            .unwrap_or("2025-06-18");
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": { "listChanged": true } },
            "serverInfo": { "name": "studio", "version": env!("CARGO_PKG_VERSION") },
            "instructions": tools::INSTRUCTIONS,
        })
    }

    /// Ask Studio for each action tool's schema it has not given yet.
    fn learn_schemas(&self) -> bool {
        let cancel = Cancel::default();
        let mut reached = true;
        for tool in tools::curated() {
            let Exec::Action(id) = tool.exec else {
                continue;
            };
            if self
                .schemas
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .contains_key(id)
            {
                continue;
            }
            match self.up.call("describe", &json!({ "id": id }), &cancel) {
                Ok(d) => {
                    let mut takes = d
                        .get("takes")
                        .cloned()
                        .filter(Value::is_object)
                        .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
                    tools::fix_schema(id, &mut takes);
                    let effect = d
                        .get("effect")
                        .and_then(Value::as_str)
                        .unwrap_or("mutating")
                        .to_owned();
                    self.schemas
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert(id, (takes, effect));
                }
                Err(e) if e.get("code").and_then(Value::as_str) == Some("unavailable") => {
                    reached = false;
                    break;
                }
                // Not in this Studio (SIGNAL_BLOCK_SET before it lands):
                // listed with the bridge's own schema.
                Err(_) => {}
            }
        }
        reached
    }

    fn schema_of(&self, tool: &Tool) -> (Value, Option<String>) {
        let wait = json!({"type": "number", "minimum": 0, "maximum": 600, "description": "Seconds to wait before answering as a job (default 20)."});
        let (mut schema, effect) = match (tool.exec, tool.schema) {
            (_, Some(f)) => (f(), None),
            (Exec::Action(id), None) => {
                let known = self
                    .schemas
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get(id)
                    .cloned();
                match known {
                    Some((takes, effect)) => (takes, Some(effect)),
                    None => (
                        json!({"type": "object", "properties": {}, "additionalProperties": true}),
                        None,
                    ),
                }
            }
            (Exec::Composite(_), None) => (json!({"type": "object", "properties": {}}), None),
        };
        if let Exec::Action(id) = tool.exec
            && let (Some(props), Some(extra)) = (
                schema.get_mut("properties").and_then(Value::as_object_mut),
                tools::extras(id).as_object(),
            )
        {
            for (k, v) in extra {
                props.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        if let Some(props) = schema.get_mut("properties").and_then(Value::as_object_mut) {
            props.insert("wait".into(), wait);
            // A change the song may have moved on under.
            if matches!(tool.exec, Exec::Action(_))
                && effect.as_deref().is_some_and(|e| e != "read-only")
            {
                props.insert("if_revision".into(), tools::if_revision_schema());
            }
        }
        if let Some(o) = schema.as_object_mut() {
            o.remove("description");
            o.entry("type").or_insert(json!("object"));
        }
        (schema, effect)
    }

    fn list(&self) -> Vec<Value> {
        let reached = self.learn_schemas();
        self.listed_without.store(!reached, Ordering::SeqCst);
        let mut out: Vec<Value> = tools::all()
            .iter()
            .map(|t| {
                let (schema, effect) = self.schema_of(t);
                // The composites' effect is the bridge's to say.
                let effect = effect.or_else(|| match t.name {
                    "block_measure" | "balance" | "actions" | "describe" | "watch" => {
                        Some("read-only".to_owned())
                    }
                    "run" | "batch" | "script" => Some("mutating".to_owned()),
                    _ => None,
                });
                let mut tool = json!({ "name": t.name, "description": t.description, "inputSchema": schema });
                if let (Some(e), Some(o)) = (effect, tool.as_object_mut()) {
                    o.insert(
                        "annotations".into(),
                        json!({ "readOnlyHint": e == "read-only", "destructiveHint": e == "destructive" }),
                    );
                }
                tool
            })
            .collect();
        out.extend(
            tools::bridge_tools()
                .into_iter()
                .map(|(name, description, schema)| {
                    json!({ "name": name, "description": description, "inputSchema": schema,
                    "annotations": { "readOnlyHint": name != "job_cancel" } })
                }),
        );
        out
    }

    /// After a call reached Studio: if the tool list was given without
    /// it, it can now say what each tool takes.
    fn reached(&self) {
        if self.listed_without.swap(false, Ordering::SeqCst) && self.learn_schemas() {
            (self.notify)(
                json!({ "jsonrpc": "2.0", "method": "notifications/tools/list_changed" }),
            );
        }
    }

    fn call(&self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let mut args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if args.is_null() {
            args = json!({});
        }
        if !args.is_object() {
            return Err((-32_602, "arguments must be an object".into()));
        }
        let token = params
            .get("_meta")
            .and_then(|m| m.get("progressToken"))
            .cloned();
        match name {
            "job" => return Ok(self.job(&args, token.as_ref())),
            "jobs" => return Ok(ok(&json!({ "jobs": self.jobs.list() }))),
            "job_cancel" => return Ok(self.job_cancel(&args)),
            "more" => return Ok(self.more(&args)),
            _ => {}
        }
        let all = tools::all();
        let Some(tool) = all.iter().find(|t| t.name == name).copied() else {
            let mut names: Vec<&str> = all.iter().map(|t| t.name).collect();
            names.extend(tools::bridge_tools().iter().map(|t| t.0));
            return Err((
                -32_602,
                format!(
                    "no tool {name}; closest: {}",
                    closest(name, &names, 3).join(", ")
                ),
            ));
        };
        let wait = wait_of(&mut args, self.config.wait);
        let up = self.up.clone();
        let agent = Arc::clone(&self.agent);
        let job = self.jobs.spawn(name, move |job: &Job| {
            let ctx = Ctx {
                up: up.as_ref(),
                job,
                agent: agent.as_ref(),
                step: None,
            };
            match tool.exec {
                Exec::Action(id) => tools::run_action(&ctx, id, &args),
                Exec::Composite(f) => f(&ctx, &args),
            }
        });
        Ok(self.answer(&job, wait, token.as_ref()))
    }

    /// Wait for `job` up to `wait`, telling the client it goes on; its
    /// answer, or how it stands.
    fn answer(&self, job: &Arc<Job>, wait: Duration, token: Option<&Value>) -> Value {
        let slice = Duration::from_secs(5);
        let mut left = wait;
        let mut beat = 0u64;
        let state = loop {
            let now = left.min(slice);
            let state = job.wait(now);
            left = left.saturating_sub(now);
            if !matches!(state, State::Running) || left.is_zero() {
                break state;
            }
            if let Some(t) = token {
                beat = beat.saturating_add(1);
                (self.notify)(
                    json!({ "jsonrpc": "2.0", "method": "notifications/progress", "params": {
                        "progressToken": t, "progress": beat,
                        "message": format!("{} running, {}", job.tool, job.status().get("elapsed_s").cloned().unwrap_or_default()),
                    }}),
                );
            }
        };
        match state {
            State::Done(Ok(v)) => {
                self.reached();
                // Every answer says the song's revision it read.
                let v = crate::agent::with_revision(v, job.revision());
                ok(&self.pages.fit(v, self.config.budget))
            }
            State::Done(Err(mut f)) => {
                if f.get("code").and_then(Value::as_str) != Some("unavailable") {
                    self.reached();
                }
                if let (Some(r), Some(o)) = (job.revision(), f.as_object_mut()) {
                    o.entry("revision").or_insert(json!(r));
                }
                err(&self.enrich(f))
            }
            State::Cancelled => err(&failure(
                "cancelled",
                format!("job {} was cancelled", job.id),
            )),
            State::Running => {
                job.background.store(true, Ordering::SeqCst);
                let mut s = job.status();
                if let Some(o) = s.as_object_mut() {
                    o.insert(
                        "next".into(),
                        json!(format!("still running: job {{\"job\": \"{}\"}} waits for its result (job_cancel to stop)", job.id)),
                    );
                }
                ok(&s)
            }
        }
    }

    fn job(&self, args: &Value, token: Option<&Value>) -> Value {
        let Some(id) = args.get("job").and_then(Value::as_str) else {
            return err(&failure(
                "bad_args",
                "`job` is missing (its id, `j3`; `jobs` lists them)",
            ));
        };
        match self.jobs.get(id) {
            Ok(job) => {
                let mut a = args.clone();
                let wait = wait_of(&mut a, self.config.wait);
                self.answer(&job, wait, token)
            }
            Err(e) => err(&failure("unknown", e)),
        }
    }

    fn job_cancel(&self, args: &Value) -> Value {
        let id = args.get("job").and_then(Value::as_str).unwrap_or_default();
        match self.jobs.get(id) {
            Ok(job) => {
                job.cancel();
                ok(&job.status())
            }
            Err(e) => err(&failure("unknown", e)),
        }
    }

    fn more(&self, args: &Value) -> Value {
        let id = args
            .get("result")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let offset = args
            .get("offset")
            .and_then(Value::as_u64)
            .and_then(|o| usize::try_from(o).ok())
            .unwrap_or(0);
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .and_then(|l| usize::try_from(l).ok());
        match self.pages.more(id, offset, limit, self.config.budget) {
            Ok(v) => ok(&v),
            Err(e) => err(&failure("unknown", e)),
        }
    }

    /// A failure with what to do next: the closest track paths for a
    /// track that is not there, the closest ids for an unknown action.
    fn enrich(&self, mut f: Failure) -> Failure {
        let message = f
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let cancel = Cancel::default();
        let suggestions = message.split("no track ").nth(1).map_or_else(
            || {
                let unknown = f.get("code").and_then(Value::as_str) == Some("unknown")
                    && message.starts_with("no action ");
                if !unknown {
                    return None;
                }
                let wanted = message
                    .trim_start_matches("no action ")
                    .split([' ', '—'])
                    .next()
                    .unwrap_or_default();
                self.up
                    .call("actions", &json!({}), &cancel)
                    .ok()
                    .map(|all| {
                        let ids: Vec<&str> = all
                            .get("actions")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(|a| a.get("id").and_then(Value::as_str))
                            .collect();
                        closest(wanted, &ids, 5)
                    })
            },
            |rest| {
                let wanted = rest
                    .split([' ', '—', ',', ';'])
                    .next()
                    .unwrap_or_default()
                    .trim_matches(['`', '"', '\'']);
                self.up
                    .call("run", &json!({"id": "SESSION_TRACKS", "args": {}}), &cancel)
                    .ok()
                    .map(|all| {
                        let paths: Vec<&str> = all
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|t| t.get("path").and_then(Value::as_str))
                            .collect();
                        closest_path(wanted, &paths, 5)
                    })
            },
        );
        if let (Some(s), Some(o)) = (suggestions, f.as_object_mut())
            && !s.is_empty()
        {
            o.insert("did_you_mean".into(), json!(s));
        }
        f
    }
}

/// `wait` (seconds) taken out of a call's arguments.
fn wait_of(args: &mut Value, default: Duration) -> Duration {
    args.as_object_mut()
        .and_then(|o| o.remove("wait"))
        .and_then(|w| w.as_f64())
        .filter(|w| w.is_finite() && *w >= 0.0)
        .map_or(default, |w| Duration::from_secs_f64(w.min(600.0)))
}

fn ok(v: &Value) -> Value {
    json!({ "content": [{ "type": "text", "text": v.to_string() }], "isError": false })
}

fn err(f: &Failure) -> Value {
    json!({ "content": [{ "type": "text", "text": f.to_string() }], "isError": true })
}

/// Edit distance, case aside.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.to_lowercase().chars().collect();
    let b: Vec<char> = b.to_lowercase().chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i.saturating_add(1)];
        for (j, cb) in b.iter().enumerate() {
            let sub = prev
                .get(j)
                .copied()
                .unwrap_or(0)
                .saturating_add(usize::from(ca != cb));
            let del = prev
                .get(j.saturating_add(1))
                .copied()
                .unwrap_or(0)
                .saturating_add(1);
            let ins = cur.get(j).copied().unwrap_or(0).saturating_add(1);
            cur.push(sub.min(del).min(ins));
        }
        prev = cur;
    }
    prev.last().copied().unwrap_or(0)
}

/// The `n` names closest to `wanted`.
#[must_use]
pub fn closest(wanted: &str, names: &[&str], n: usize) -> Vec<String> {
    let mut scored: Vec<(usize, &str)> = names
        .iter()
        .map(|c| {
            let contains = c.to_lowercase().contains(&wanted.to_lowercase());
            (if contains { 0 } else { distance(wanted, c) }, *c)
        })
        .collect();
    scored.sort_by_key(|(d, c)| (*d, c.len()));
    scored
        .into_iter()
        .take(n)
        .map(|(_, c)| c.to_owned())
        .collect()
}

/// The `n` track paths closest to `wanted`: against each tail of a path
/// (`Sum/In`, `Kick/Sum/In`), since a tail addresses it.
#[must_use]
pub fn closest_path(wanted: &str, paths: &[&str], n: usize) -> Vec<String> {
    let mut scored: Vec<(usize, &str)> = paths
        .iter()
        .map(|p| {
            let parts: Vec<&str> = p.split('/').collect();
            let best = (0..parts.len())
                .filter_map(|i| parts.get(i..).map(|tail| distance(wanted, &tail.join("/"))))
                .min()
                .unwrap_or(usize::MAX);
            (best, *p)
        })
        .collect();
    scored.sort_by_key(|(d, p)| (*d, p.len()));
    scored
        .into_iter()
        .take(n)
        .map(|(_, p)| p.to_owned())
        .collect()
}

/// Serve `bridge` on stdin/stdout until stdin ends: each request on a
/// thread of its own, so a call waiting on a render never holds up
/// another.
///
/// # Errors
///
/// Reading stdin failed.
pub fn serve_stdio(bridge: &Arc<Bridge>, out: &Arc<Mutex<std::io::Stdout>>) -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut calls: Vec<std::thread::JoinHandle<()>> = Vec::new();
    for line in stdin.lock().lines() {
        calls.retain(|c| !c.is_finished());
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(e) => {
                write_line(
                    out,
                    &json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32_700, "message": format!("not JSON: {e}")}}),
                );
                continue;
            }
        };
        let bridge = bridge.clone();
        let to = out.clone();
        let spawned = std::thread::Builder::new()
            .name("studio-mcp call".into())
            .spawn(move || {
                if let Some(reply) = bridge.handle(&msg) {
                    write_line(&to, &reply);
                }
            });
        match spawned {
            Ok(call) => calls.push(call),
            Err(e) => write_line(
                out,
                &json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32_603, "message": format!("could not answer: {e}")}}),
            ),
        }
    }
    // The client is done asking: answer what it asked before going.
    for call in calls {
        let _ = call.join();
    }
    Ok(())
}

/// One message on stdout, whole.
pub fn write_line(out: &Arc<Mutex<std::io::Stdout>>, v: &Value) {
    let mut o = out.lock().unwrap_or_else(PoisonError::into_inner);
    let mut line = v.to_string();
    line.push('\n');
    let _ = o.write_all(line.as_bytes());
    let _ = o.flush();
}
