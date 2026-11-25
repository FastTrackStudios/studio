//! The bridge against a fake Studio on a real control socket: finding
//! it, surviving its restart, the typed tools, compact answers, pages and
//! jobs.
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::needless_pass_by_value
)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use studio_mcp::server::Notify;
use studio_mcp::upstream::{Socket, socket_path};
use studio_mcp::{Bridge, Config};

/// A fake Studio: answers `describe`, `actions` and a few `run`s.
struct Fake {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    path: PathBuf,
}

fn signal_chain() -> Value {
    let params: Vec<Value> = (0..170)
        .map(|i| json!({"id": format!("band.{i}.gain"), "default": 0.0, "value": if i == 3 { -4.0 } else { 0.0 },
                         "text": if i == 3 { "-4.0 dB" } else { "0.0 dB" }, "choices": ["Bell", "Shelf", "Low Cut", "High Cut"]}))
        .collect();
    json!({"mode": "per-track", "phases": [{"id": "tone", "makeup_db": 0.0, "blocks": [
        {"id": "tone.eq", "kind": "eq", "params": params},
        {"id": "tone.comp", "kind": "comp", "params": [{"id": "threshold", "default": -18.0, "value": -24.0, "text": "-24.0 dB"}]}]}]})
}

fn answer(method: &str, params: &Value) -> Value {
    let id = params.get("id").and_then(Value::as_str).unwrap_or_default();
    let args = params.get("args").cloned().unwrap_or(Value::Null);
    match (method, id) {
        ("describe", "MIX_STATE") => json!({"result": {"id": "MIX_STATE", "effect": "read-only",
            "takes": {"type": "object", "description": "What to read.", "properties": {"track": {"type": "string"}, "depth": {"type": "integer"}, "params": {"type": "boolean"}}, "required": ["track"]}}}),
        ("describe", "TRACK_SET") => json!({"result": {"id": "TRACK_SET", "effect": "mutating",
            "takes": {"type": "object", "properties": {"track": {"type": "string"}, "volume_db": {"type": "number"}}, "required": ["track"]}}}),
        ("actions", _) => json!({"result": {"actions": [
            {"id": "MIX_STATE", "name": "Everything about a track", "effect": "read-only", "enabled": true},
            {"id": "MIX_MEASURE", "name": "Measure the mix", "effect": "read-only", "enabled": true}]}}),
        ("run", "MIX_STATE") => {
            if args.get("track").and_then(Value::as_str) != Some("Drums") {
                return json!({"error": {"code": "failed", "error": format!("MIX_STATE: no track {} — list them with SESSION_TRACKS", args["track"].as_str().unwrap_or(""))}});
            }
            json!({"result": {"track": "Drums", "from": args.get("params"), "tracks": [
                {"path": "Drums", "kind": "folder", "level": 0, "volume_db": -9.24, "pan": 0.0, "mute": false, "solo": false,
                 "fx": [{"place": "#1", "name": "Signal Dev", "on": true, "latency": 0, "signal": signal_chain()}]}]}})
        }
        ("run", "SESSION_TRACKS") => json!({"result": (0..300).map(|i| json!({
            "guid": format!("{{{i:08}-0000-0000-0000-000000000000}}"), "name": format!("T{i}"),
            "path": if i == 0 { "Drums".to_owned() } else { format!("Drums/Kick/T{i}") }, "folder": i == 0,
            "muted": false, "soloed": false, "inverted": false, "volume_db": -1.5, "pan": 0.0, "items": 3,
            "fx": ["Signal Dev"], "lanes": [{"lane": 0, "name": "1", "items": 3, "plays": true}], "armed": false, "record_input": "none"
        })).collect::<Vec<_>>()}),
        ("run", "SLOW") => {
            std::thread::sleep(Duration::from_millis(1500));
            json!({"result": {"done": true}})
        }
        ("run", "SIGNAL_CHAIN") => {
            json!({"result": {"track": "Drums/Kick/Sum/In", "chain": signal_chain()}})
        }
        ("run", "LISTEN") => {
            let at = args["at"].as_str().unwrap_or_default();
            let (lufs, crest) = if at.ends_with(":pre") {
                (-20.0, 18.0)
            } else {
                (-22.0, 12.0)
            };
            json!({"result": {"track": "Drums/Kick/Sum/In", "from": 73.5, "to": 85.5,
                "figures": {"lufs": lufs, "crest": crest, "peak": -1.0, "short_term": [lufs, lufs]}}})
        }
        _ => {
            json!({"error": {"code": "unknown", "error": format!("no action {id} — find it with `actions`")}})
        }
    }
}

impl Fake {
    fn start(path: &Path) -> Self {
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let s = stop.clone();
        let thread = std::thread::spawn(move || {
            while !s.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((conn, _)) => {
                        conn.set_nonblocking(false).unwrap();
                        std::thread::spawn(move || {
                            let mut reader = BufReader::new(conn.try_clone().unwrap());
                            let mut w = conn;
                            let mut line = String::new();
                            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                                let req: Value = serde_json::from_str(&line).unwrap();
                                let mut a = answer(req["method"].as_str().unwrap(), &req["params"]);
                                a["id"] = req["id"].clone();
                                let _ = writeln!(w, "{a}");
                                line.clear();
                            }
                        });
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        });
        Self {
            stop,
            thread: Some(thread),
            path: path.to_path_buf(),
        }
    }

    fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("studio-mcp-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

struct Session {
    bridge: Bridge,
    notes: Arc<Mutex<Vec<Value>>>,
}

fn session(path: &Path, wait_s: f64) -> Session {
    let notes = Arc::new(Mutex::new(Vec::new()));
    let n = notes.clone();
    let notify: Notify = Arc::new(move |v| n.lock().unwrap().push(v));
    let up = Arc::new(Socket::new(path.to_path_buf(), Duration::from_secs(3)));
    let config = Config {
        wait: Duration::from_secs_f64(wait_s),
        budget: 8000,
    };
    Session {
        bridge: Bridge::new(up, config, notify),
        notes,
    }
}

impl Session {
    fn call(&self, name: &str, args: Value) -> (bool, Value) {
        let reply = self
            .bridge
            .handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": args}}))
            .unwrap();
        let result = &reply["result"];
        assert!(!result.is_null(), "{reply}");
        let text = result["content"][0]["text"].as_str().unwrap();
        (
            result["isError"] == json!(true),
            serde_json::from_str(text).unwrap(),
        )
    }

    fn tools(&self) -> Vec<Value> {
        let reply = self
            .bridge
            .handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
            .unwrap();
        reply["result"]["tools"].as_array().unwrap().clone()
    }
}

#[test]
fn the_socket_is_found_where_architect_control_puts_it() {
    let env = |vars: &'static [(&'static str, &'static str)]| {
        move |k: &str| {
            vars.iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| (*v).to_owned())
        }
    };
    assert_eq!(
        socket_path(
            "studio",
            None,
            &env(&[("XDG_RUNTIME_DIR", "/run/user/1000")])
        ),
        PathBuf::from("/run/user/1000/fts/studio.sock")
    );
    assert_eq!(
        socket_path(
            "studio",
            None,
            &env(&[
                ("XDG_RUNTIME_DIR", "/run/user/1000"),
                ("FTS_CONTROL_DIR", "/tmp/c")
            ])
        ),
        PathBuf::from("/tmp/c/studio.sock")
    );
    assert_eq!(
        socket_path(
            "studio",
            None,
            &env(&[
                ("STUDIO_MCP_SOCKET", "/x.sock"),
                ("FTS_CONTROL_DIR", "/tmp/c")
            ])
        ),
        PathBuf::from("/x.sock")
    );
    assert_eq!(
        socket_path(
            "studio",
            Some(Path::new("/y.sock")),
            &env(&[("STUDIO_MCP_SOCKET", "/x.sock")])
        ),
        PathBuf::from("/y.sock")
    );
}

#[test]
fn calls_carry_on_across_a_studio_restart() {
    let path = dir("restart").join("studio.sock");
    let fake = Fake::start(&path);
    let s = session(&path, 10.0);
    assert!(!s.call("mix_state", json!({"track": "Drums"})).0);
    fake.stop();
    // Studio comes back while a call is waiting for it.
    let p = path;
    let back = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(600));
        Fake::start(&p)
    });
    let (error, v) = s.call("mix_state", json!({"track": "Drums"}));
    assert!(!error, "{v}");
    back.join().unwrap().stop();
}

#[test]
fn studio_down_says_how_to_start_it() {
    let path = dir("down").join("studio.sock");
    let s = session(&path, 10.0);
    let (error, v) = s.call("mix_state", json!({"track": "Drums"}));
    assert!(error);
    assert_eq!(v["code"], json!("unavailable"));
    assert!(
        v["error"].as_str().unwrap().contains("finds it again"),
        "{v}"
    );
}

#[test]
fn tools_take_their_actions_schemas_and_say_when_they_learn_them() {
    let path = dir("schemas").join("studio.sock");
    let s = session(&path, 10.0);
    // Listed while Studio is down: a loose schema.
    let t = s.tools();
    let mix = t.iter().find(|t| t["name"] == "mix_state").unwrap();
    assert!(mix["inputSchema"]["properties"]["track"].is_null());
    let fake = Fake::start(&path);
    assert!(!s.call("mix_state", json!({"track": "Drums"})).0);
    assert!(
        s.notes
            .lock()
            .unwrap()
            .iter()
            .any(|n| n["method"] == "notifications/tools/list_changed")
    );
    let t = s.tools();
    let mix = t.iter().find(|t| t["name"] == "mix_state").unwrap();
    assert_eq!(mix["inputSchema"]["required"], json!(["track"]));
    assert!(mix["inputSchema"]["properties"]["detail"].is_object());
    assert!(mix["inputSchema"]["properties"]["wait"].is_object());
    assert_eq!(mix["annotations"]["readOnlyHint"], json!(true));
    let set = t.iter().find(|t| t["name"] == "track_set").unwrap();
    assert_eq!(set["annotations"]["readOnlyHint"], json!(false));
    for name in [
        "job",
        "jobs",
        "job_cancel",
        "more",
        "run",
        "actions",
        "balance",
        "block_measure",
    ] {
        assert!(t.iter().any(|t| t["name"] == name), "{name}");
    }
    fake.stop();
}

#[test]
fn mix_state_is_compact_unless_asked() {
    let path = dir("compact").join("studio.sock");
    let fake = Fake::start(&path);
    let s = session(&path, 10.0);
    let (_, v) = s.call("mix_state", json!({"track": "Drums"}));
    // A summary asks Studio for no FX parameters (the fake echoes `params` as `from`).
    assert_eq!(v["from"], json!(false));
    let t = &v["tracks"][0];
    assert_eq!(t["fx"], json!(["#1 Signal Dev"]));
    assert_eq!(
        t["signal"]["phases"][0]["blocks"][0]["set"],
        json!({"band.3.gain": "-4.0 dB"})
    );
    assert!(v.to_string().len() < 1000, "{}", v.to_string().len());
    // Full: the raw answer, too big for the budget, so paged.
    let (_, full) = s.call("mix_state", json!({"track": "Drums", "detail": "full"}));
    assert!(
        full["page"]["result"].is_string(),
        "{}",
        full.to_string().len()
    );
    fake.stop();
}

#[test]
fn the_session_tree_is_small_and_pages_when_it_is_not() {
    let path = dir("tree").join("studio.sock");
    let fake = Fake::start(&path);
    let s = session(&path, 10.0);
    let (_, v) = s.call("session_tree", json!({}));
    assert_eq!(v["tracks"], json!(300));
    assert_eq!(
        v["tree"][1],
        json!("Drums/Kick/T1  -1.5 dB  fx: Signal Dev  items 3")
    );
    let first = v["tree"].as_array().unwrap().len();
    assert!(first < 300);
    let id = v["page"]["result"].as_str().unwrap();
    let (_, next) = s.call("more", json!({"result": id, "offset": first}));
    assert_eq!(
        next["tree"][0],
        json!(format!(
            "Drums/Kick/T{first}  -1.5 dB  fx: Signal Dev  items 3"
        ))
    );
    let (_, list) = s.call(
        "session_tree",
        json!({"view": "list", "fields": ["path"], "under": "Drums/Kick"}),
    );
    // 299 short objects fit the budget: the list itself, unpaged.
    assert_eq!(list[0], json!({"path": "Drums/Kick/T1"}));
    fake.stop();
}

#[test]
fn a_long_call_becomes_a_job_and_is_read_when_done() {
    let path = dir("jobs").join("studio.sock");
    let fake = Fake::start(&path);
    let s = session(&path, 0.2);
    let (error, v) = s.call("run", json!({"id": "SLOW"}));
    assert!(!error);
    assert_eq!(v["state"], json!("running"));
    let job = v["job"].as_str().unwrap().to_owned();
    let (_, listed) = s.call("jobs", json!({}));
    assert_eq!(listed["jobs"][0]["job"], json!(job));
    let (_, still) = s.call("job", json!({"job": job, "wait": 0}));
    assert_eq!(still["state"], json!("running"));
    let (_, done) = s.call("job", json!({"job": job, "wait": 5}));
    assert_eq!(done, json!({"done": true}));
    fake.stop();
}

#[test]
fn a_cancelled_job_stops_at_once() {
    let path = dir("cancel").join("studio.sock");
    let fake = Fake::start(&path);
    let s = session(&path, 0.1);
    let (_, v) = s.call("run", json!({"id": "SLOW"}));
    let job = v["job"].as_str().unwrap().to_owned();
    let (_, c) = s.call("job_cancel", json!({"job": job}));
    assert_eq!(c["state"], json!("cancelled"));
    let (error, after) = s.call("job", json!({"job": job}));
    assert!(error);
    assert_eq!(after["code"], json!("cancelled"));
    fake.stop();
}

#[test]
fn a_blocks_effect_is_one_call() {
    let path = dir("block").join("studio.sock");
    let fake = Fake::start(&path);
    let s = session(&path, 10.0);
    let (error, v) = s.call(
        "block_measure",
        json!({"track": "Kick/Sum/In", "block": "tone.comp", "from": "CH 1", "to": "CH 1 end"}),
    );
    assert!(!error, "{v}");
    assert_eq!(v["gain_db"], json!(-2.0));
    assert_eq!(v["crest_db"], json!(-6.0));
    let (_, b) = s.call(
        "signal_block",
        json!({"track": "Kick/Sum/In", "block": "comp"}),
    );
    assert_eq!(
        b["block"]["params"],
        json!(["threshold = -24.0 dB (default -18)"])
    );
    let (error, none) = s.call(
        "signal_block",
        json!({"track": "Kick/Sum/In", "block": "gate"}),
    );
    assert!(error);
    assert!(
        none["error"]
            .as_str()
            .unwrap()
            .contains("tone.eq, tone.comp"),
        "{none}"
    );
    fake.stop();
}

#[test]
fn errors_say_what_was_meant() {
    let path = dir("errors").join("studio.sock");
    let fake = Fake::start(&path);
    let s = session(&path, 10.0);
    let (error, v) = s.call("mix_state", json!({"track": "Kik/T12"}));
    assert!(error);
    assert_eq!(v["did_you_mean"][0], json!("Drums/Kick/T12"), "{v}");
    let (_, v) = s.call("run", json!({"id": "MIX_MEASUR"}));
    assert_eq!(v["did_you_mean"][0], json!("MIX_MEASURE"), "{v}");
    let reply = s
        .bridge
        .handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "mix_stat", "arguments": {}}}))
        .unwrap();
    assert!(
        reply["error"]["message"]
            .as_str()
            .unwrap()
            .contains("mix_state"),
        "{reply}"
    );
    fake.stop();
}
