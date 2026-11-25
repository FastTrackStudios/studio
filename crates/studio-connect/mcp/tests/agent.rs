//! The agent beside the person, on a fake Studio that speaks the engine's
//! `AGENT_RUN`: every answer says the song's revision, an edit made at an
//! old one is refused `stale`, `changes` lists both sides' edits, and a
//! script's (or batch's) edits are one undo step.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::needless_pass_by_value
)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use studio_mcp::server::Notify;
use studio_mcp::upstream::{Cancel, Failure, Upstream};
use studio_mcp::{Bridge, Config};

/// One edit in the fake song.
#[derive(Clone, Debug)]
struct Edit {
    revision: u64,
    by: &'static str,
    action: String,
    /// The undo step it is part of, when an agent's step was open.
    step: Option<String>,
}

/// A fake engine: a fader, its journal, and undo steps.
#[derive(Default)]
struct Engine {
    volume_db: f64,
    edits: Vec<Edit>,
    /// The agent's undo step being made.
    open_step: Option<String>,
    /// The undo stack: each entry an undo step's name and the volume
    /// before it.
    undo: Vec<(String, f64)>,
}

#[derive(Default)]
struct Fake {
    engine: Mutex<Engine>,
    calls: Mutex<Vec<String>>,
}

impl Fake {
    fn revision(&self) -> u64 {
        self.engine.lock().unwrap().edits.len() as u64
    }

    /// The person moves the fader in the window.
    fn person_sets(&self, db: f64) {
        let mut e = self.engine.lock().unwrap();
        let before = e.volume_db;
        e.volume_db = db;
        let revision = e.edits.len() as u64 + 1;
        e.edits.push(Edit {
            revision,
            by: "person",
            action: "TRACK_SET".into(),
            step: None,
        });
        e.undo.push(("Set track".into(), before));
    }

    /// An action, run as the engine runs it: its answer.
    fn action(&self, id: &str, args: &Value, by: &'static str) -> Result<Value, Value> {
        let mut e = self.engine.lock().unwrap();
        match id {
            "MIX_STATE" => Ok(json!({"track": "Kick", "volume_db": e.volume_db})),
            "TRACK_SET" => {
                let before = e.volume_db;
                e.volume_db = args["volume_db"].as_f64().unwrap_or(before);
                let revision = e.edits.len() as u64 + 1;
                let step = e.open_step.clone();
                e.edits.push(Edit {
                    revision,
                    by,
                    action: id.into(),
                    step: step.clone(),
                });
                // Within an open step, one undo entry for all of it.
                match step {
                    Some(s) if e.undo.last().is_some_and(|(n, _)| *n == s) => {}
                    Some(s) => e.undo.push((s, before)),
                    None => e.undo.push(("Set track".into(), before)),
                }
                Ok(json!(null))
            }
            "SESSION_UNDO" => {
                let Some((name, before)) = e.undo.pop() else {
                    return Ok(json!("nothing to undo"));
                };
                e.volume_db = before;
                Ok(json!(format!("undid {name}")))
            }
            _ => Err(json!({"code": "unknown", "error": format!("no action {id}")})),
        }
    }
}

impl Upstream for Fake {
    fn call(&self, method: &str, params: &Value, _cancel: &Cancel) -> Result<Value, Failure> {
        let id = params["id"].as_str().unwrap_or_default().to_owned();
        let args = params.get("args").cloned().unwrap_or(Value::Null);
        self.calls.lock().unwrap().push(id.clone());
        match (method, id.as_str()) {
            ("describe", "TRACK_SET") => Ok(
                json!({"id": id, "effect": "mutating", "takes": {"type": "object", "properties": {"track": {"type": "string"}, "volume_db": {"type": "number"}}}}),
            ),
            ("describe", _) => Ok(
                json!({"id": id, "effect": "read-only", "takes": {"type": "object", "properties": {}}}),
            ),
            ("run", "AGENT_RUN") => {
                let inner = args["id"].as_str().unwrap_or_default();
                if let Some(want) = args["if_revision"].as_u64() {
                    let now = self.revision();
                    if want != now {
                        let e = self.engine.lock().unwrap();
                        let changes: Vec<Value> = e
                            .edits
                            .iter()
                            .filter(|c| c.revision > want)
                            .map(
                                |c| json!({"revision": c.revision, "by": c.by, "action": c.action}),
                            )
                            .collect();
                        return Ok(
                            json!({"ok": false, "code": "stale", "error": format!("stale: the song moved on from revision {want} to {now}"),
                            "revision": now, "since": want, "changes": changes}),
                        );
                    }
                }
                if let Some(step) = args["step"].as_str()
                    && inner == "TRACK_SET"
                {
                    self.engine.lock().unwrap().open_step = Some(step.to_owned());
                }
                match self.action(inner, &args["args"], "agent") {
                    Ok(answer) => {
                        Ok(json!({"ok": true, "answer": answer, "revision": self.revision()}))
                    }
                    Err(f) => Ok(
                        json!({"ok": false, "code": f["code"], "error": f["error"], "revision": self.revision()}),
                    ),
                }
            }
            ("run", "AGENT_STEP_END") => {
                let mut e = self.engine.lock().unwrap();
                let step = args["step"].as_str().unwrap_or_default().to_owned();
                let open = e.open_step.take();
                let steps = e
                    .edits
                    .iter()
                    .filter(|c| c.step.as_deref() == Some(&step))
                    .count();
                let undo = (open.as_deref() == Some(step.as_str()) && steps > 0).then_some(step);
                Ok(json!({"undo": undo, "steps": steps, "revision": e.edits.len()}))
            }
            ("run", "SESSION_CHANGES") => {
                let since = args["since"].as_u64().unwrap_or(0);
                let e = self.engine.lock().unwrap();
                let changes: Vec<Value> = e
                    .edits
                    .iter()
                    .filter(|c| c.revision > since)
                    .map(|c| json!({"revision": c.revision, "by": c.by, "action": c.action, "kind": "track", "track": "Kick", "fields": ["volume_db"]}))
                    .collect();
                Ok(
                    json!({"revision": e.edits.len(), "since": since, "complete": true, "total": changes.len(), "changes": changes}),
                )
            }
            ("run", other) => self.action(other, &args, "person").map_err(|f| f),
            _ => Err(json!({"code": "unknown", "error": "no such method"})),
        }
    }
}

struct Session {
    fake: Arc<Fake>,
    bridge: Bridge,
}

fn session() -> Session {
    let fake = Arc::new(Fake::default());
    let notify: Notify = Arc::new(|_| {});
    let config = Config {
        wait: Duration::from_secs(10),
        budget: 8000,
    };
    Session {
        bridge: Bridge::new(fake.clone(), config, notify),
        fake,
    }
}

impl Session {
    fn call(&self, name: &str, args: Value) -> (bool, Value) {
        let reply = self
            .bridge
            .handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": args}}))
            .unwrap();
        let result = &reply["result"];
        let text = result["content"][0]["text"].as_str().unwrap();
        (
            result["isError"] == json!(true),
            serde_json::from_str(text).unwrap(),
        )
    }
}

/// A read says the song's revision; an edit passing it back runs. After
/// the person moves the fader, the same edit is refused `stale` — nothing
/// run, the person's edit named — and runs once retried at the new one.
#[test]
fn an_edit_at_a_revision_the_person_moved_past_is_refused_stale() {
    let s = session();
    let (error, read) = s.call("mix_state", json!({"track": "Kick", "detail": "full"}));
    assert!(!error, "{read}");
    assert_eq!(read["revision"], 0);
    let (error, set) = s.call(
        "track_set",
        json!({"track": "Kick", "volume_db": -3.0, "if_revision": 0}),
    );
    assert!(!error, "{set}");
    assert_eq!(
        set["revision"], 1,
        "the edit's answer says where it left the song"
    );

    s.fake.person_sets(-12.0);
    let (error, stale) = s.call(
        "track_set",
        json!({"track": "Kick", "volume_db": -6.0, "if_revision": 1}),
    );
    assert!(error, "{stale}");
    assert_eq!(stale["code"], "stale");
    assert_eq!(stale["revision"], 2);
    assert_eq!(stale["changes"][0]["by"], "person");
    assert_eq!(
        s.fake.engine.lock().unwrap().volume_db,
        -12.0,
        "nothing ran"
    );

    let (error, retried) = s.call(
        "track_set",
        json!({"track": "Kick", "volume_db": -6.0, "if_revision": 2}),
    );
    assert!(!error, "{retried}");
    assert_eq!(s.fake.engine.lock().unwrap().volume_db, -6.0);
}

/// `run` and a batch's first step take `if_revision` too; a script's call
/// throws `stale`.
#[test]
fn run_batch_and_script_refuse_a_stale_edit_too() {
    let s = session();
    s.fake.person_sets(-1.0);
    let (error, v) = s.call(
        "run",
        json!({"id": "TRACK_SET", "args": {"track": "Kick", "volume_db": 0.0}, "if_revision": 0}),
    );
    assert!(error && v["code"] == "stale", "{v}");
    let (_, b) = s.call("batch", json!({"if_revision": 0, "steps": [{"id": "TRACK_SET", "args": {"track": "Kick", "volume_db": 0.0}}]}));
    assert_eq!(b["results"][0]["error"]["code"], "stale", "{b}");
    let (error, sc) = s.call(
        "script",
        json!({"source": "try { await studio.trackSet({track: 'Kick', volume_db: 0, if_revision: 0}); } catch (e) { return e.code; }"}),
    );
    assert!(!error, "{sc}");
    assert_eq!(sc["result"], "stale");
    assert_eq!(s.fake.engine.lock().unwrap().volume_db, -1.0);
}

/// `changes {since}` lists the edits after a revision from both sides.
#[test]
fn changes_lists_the_person_and_the_agent() {
    let s = session();
    let (_, read) = s.call("mix_state", json!({"track": "Kick", "detail": "full"}));
    let since = read["revision"].as_u64().unwrap();
    s.call("track_set", json!({"track": "Kick", "volume_db": -3.0}));
    s.fake.person_sets(-4.0);
    let (error, c) = s.call("changes", json!({"since": since}));
    assert!(!error, "{c}");
    let by: Vec<&str> = c["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["by"].as_str().unwrap())
        .collect();
    assert_eq!(by, ["agent", "person"]);
    assert_eq!(c["revision"], 2);
}

/// A script's three fader moves are one undo step named for it: one
/// SESSION_UNDO puts the fader back where it was before the script.
#[test]
fn a_script_is_undone_in_one_step() {
    let s = session();
    s.fake.person_sets(-10.0);
    let (error, v) = s.call(
        "script",
        json!({"name": "tighten snare", "source": "for (const db of [-9, -8, -7]) await studio.trackSet({track: 'Kick', volume_db: db}); return 'done';"}),
    );
    assert!(!error, "{v}");
    assert_eq!(v["undo"], "Agent: tighten snare");
    assert_eq!(v["revision"], 4);
    let steps: Vec<Option<String>> = s
        .fake
        .engine
        .lock()
        .unwrap()
        .edits
        .iter()
        .map(|e| e.step.clone())
        .collect();
    assert_eq!(
        steps
            .iter()
            .filter(|x| x.as_deref() == Some("Agent: tighten snare"))
            .count(),
        3
    );
    let (_, undone) = s.call("run", json!({"id": "SESSION_UNDO"}));
    assert_eq!(undone["answer"], "undid Agent: tighten snare", "{undone}");
    assert_eq!(
        s.fake.engine.lock().unwrap().volume_db,
        -10.0,
        "the person's fader, as it was"
    );
}

/// A batch's edits are one undo step too; a read-only script makes none.
#[test]
fn a_batch_is_one_undo_step_and_a_read_makes_none() {
    let s = session();
    let (_, b) = s.call(
        "batch",
        json!({"name": "kick down", "steps": [{"id": "TRACK_SET", "args": {"track": "Kick", "volume_db": -1.0}}, {"id": "TRACK_SET", "args": {"track": "Kick", "volume_db": -2.0}}]}),
    );
    assert_eq!(b["undo"], "Agent: kick down", "{b}");
    assert_eq!(s.fake.engine.lock().unwrap().undo.len(), 1);
    let (_, r) = s.call("script", json!({"source": "return (await studio.mixState({track: 'Kick', detail: 'full'})).revision;"}));
    assert_eq!(r["undo"], Value::Null, "{r}");
    assert_eq!(r["result"], 2, "a script's read carries its revision");
}
