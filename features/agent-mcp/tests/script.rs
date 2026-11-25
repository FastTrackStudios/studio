//! The `script` tool and the bridge's own `batch`, on a fake Studio in
//! the process: the `studio` API, its answers, errors with their line,
//! renders side by side, the sandbox, the timeout and memory cap.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::float_cmp,
    clippy::needless_pass_by_value,
    clippy::arithmetic_side_effects,
    clippy::needless_raw_string_hashes
)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use studio_mcp::server::Notify;
use studio_mcp::upstream::{Cancel, Failure, Upstream};
use studio_mcp::{Bridge, Config};

/// A fake Studio: one track whose loudness follows its fader, slow
/// listens, and a record of every call.
#[derive(Default)]
struct Fake {
    gain_db: Mutex<f64>,
    calls: Mutex<Vec<(String, Value)>>,
    running: AtomicUsize,
    most: AtomicUsize,
}

fn fail(code: &str, error: &str) -> Failure {
    json!({"code": code, "error": error})
}

impl Upstream for Fake {
    fn call(&self, method: &str, params: &Value, cancel: &Cancel) -> Result<Value, Failure> {
        let id = params["id"].as_str().unwrap_or_default().to_owned();
        let args = params.get("args").cloned().unwrap_or(Value::Null);
        self.calls.lock().unwrap().push((
            if method == "run" {
                id.clone()
            } else {
                method.to_owned()
            },
            args.clone(),
        ));
        match (method, id.as_str()) {
            ("describe", "LISTEN" | "MIX_STATE" | "SESSION_TRACKS") => {
                Ok(json!({"id": id, "effect": "read-only", "takes": {"type": "object", "properties": {}}}))
            }
            ("describe", _) => {
                Ok(json!({"id": id, "effect": "mutating", "takes": {"type": "object", "properties": {}}}))
            }
            ("batch", _) => Ok(json!({"ran": 0, "of": 0, "results": [], "studio": true})),
            ("run", "LISTEN") => {
                let now = self.running.fetch_add(1, Ordering::SeqCst) + 1;
                self.most.fetch_max(now, Ordering::SeqCst);
                let ms = args["slow_ms"].as_u64().unwrap_or(0);
                let start = Instant::now();
                while start.elapsed() < Duration::from_millis(ms) && !cancel.is_cancelled() {
                    std::thread::sleep(Duration::from_millis(5));
                }
                self.running.fetch_sub(1, Ordering::SeqCst);
                if cancel.is_cancelled() {
                    return Err(fail("cancelled", "the job was cancelled"));
                }
                if args["track"] == json!("Nope") {
                    return Err(fail("failed", "LISTEN: no track Nope"));
                }
                let g = *self.gain_db.lock().unwrap();
                Ok(json!({"track": args["track"], "from": 10.0, "to": 20.0,
                    "figures": {"lufs": -20.0 + g, "crest": 14.123_456, "short_term": [1, 2, 3]}}))
            }
            ("run", "TRACK_SET") => {
                if let Some(v) = args["volume_db"].as_f64() {
                    *self.gain_db.lock().unwrap() = v;
                }
                Ok(json!({"track": args["track"], "volume_db": *self.gain_db.lock().unwrap()}))
            }
            ("run", "MIX_SNAPSHOT") => Ok(json!({"name": args["name"], "kept": true})),
            ("run", "SESSION_TRACKS") => Ok(Value::Array(
                (0..400)
                    .map(|i| json!({"path": format!("Drums/T{i}"), "name": format!("T{i}"), "folder": false, "volume_db": 0.0, "pan": 0.0, "fx": [], "items": 1}))
                    .collect(),
            )),
            ("run", _) => Err(fail("unknown", &format!("no action {id}"))),
            _ => Err(fail("unknown", "no such method")),
        }
    }
}

struct Session {
    fake: Arc<Fake>,
    bridge: Bridge,
}

fn session(wait_s: f64) -> Session {
    let fake = Arc::new(Fake::default());
    let notify: Notify = Arc::new(|_| {});
    let config = Config {
        wait: Duration::from_secs_f64(wait_s),
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
        assert!(!result.is_null(), "{reply}");
        let text = result["content"][0]["text"].as_str().unwrap();
        (
            result["isError"] == json!(true),
            serde_json::from_str(text).unwrap(),
        )
    }

    fn script(&self, source: &str) -> (bool, Value) {
        self.call("script", json!({"source": source}))
    }

    fn ran(&self, id: &str) -> Vec<Value> {
        self.fake
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(i, _)| i == id)
            .map(|(_, a)| a.clone())
            .collect()
    }
}

#[test]
fn a_script_runs_actions_and_returns_its_answer_with_its_log() {
    let s = session(10.0);
    let (error, v) = s.script(
        r#"
        const a = await studio.run("LISTEN", {track: "Kick"});
        log("kick", a.figures.lufs, {at: "post"});
        console.log("two");
        return {lufs: a.figures.lufs, args};
        "#,
    );
    assert!(!error, "{v}");
    assert_eq!(v["result"]["lufs"].as_f64(), Some(-20.0));
    assert!(v["result"]["args"].is_null());
    assert_eq!(v["log"], json!(["kick -20 {\"at\":\"post\"}", "two"]));
    assert_eq!(v["calls"], json!(1));
    assert!(v["ms"].is_u64());
}

#[test]
fn the_curated_tools_answer_as_their_tools_do() {
    let s = session(10.0);
    // `listen` drops the series and rounds, as the tool does; `run` is Studio's own answer.
    let (error, v) = s.call(
        "script",
        json!({"source": r#"
            const a = await studio.listen({track: args.track, from: "CH 1", to: "CH 1 end"});
            const raw = await studio.run("LISTEN", {track: args.track});
            return {curated: a, raw_series: raw.figures.short_term};
        "#, "args": {"track": "Kick"}}),
    );
    assert!(!error, "{v}");
    assert!(
        v["result"]["curated"]["figures"]["short_term"].is_null(),
        "{v}"
    );
    assert_eq!(v["result"]["raw_series"], json!([1, 2, 3]));
    assert_eq!(s.ran("LISTEN")[0]["from"], json!("CH 1"));
}

#[test]
fn a_script_sees_whole_answers_and_only_its_own_pages() {
    let s = session(10.0);
    let (error, v) = s.script(
        r#"
        const all = await studio.run("SESSION_TRACKS");
        return {count: all.length, tracks: all};
        "#,
    );
    assert!(!error);
    assert_eq!(v["result"]["count"], json!(400), "{v}");
    // The answer is over the budget: it pages, like any tool's.
    assert!(v["page"]["result"].is_string(), "{}", v.to_string().len());
}

#[test]
fn renders_run_side_by_side() {
    let s = session(10.0);
    let start = Instant::now();
    let (error, v) = s.script(
        r#"
        const tracks = ["Kick", "Snare", "Tom 1", "Tom 2"];
        const got = await studio.all(tracks.map((track) => studio.run("LISTEN", {track, slow_ms: 400})));
        const more = await studio.all([() => studio.run("LISTEN", {track: "OH", slow_ms: 10})]);
        return got.length + more.length;
        "#,
    );
    assert!(!error, "{v}");
    assert_eq!(v["result"], json!(5));
    assert!(
        start.elapsed() < Duration::from_millis(1400),
        "{:?}",
        start.elapsed()
    );
    assert!(s.fake.most.load(Ordering::SeqCst) >= 4);
}

#[test]
fn no_more_than_eight_calls_run_at_once() {
    let s = session(10.0);
    let (error, v) = s.script(
        r#"
        const ps = [];
        for (let i = 0; i < 20; i++) ps.push(studio.run("LISTEN", {track: "T" + i, slow_ms: 50}));
        return (await Promise.all(ps)).length;
        "#,
    );
    assert!(!error, "{v}");
    assert_eq!(v["result"], json!(20));
    assert_eq!(v["calls"], json!(20));
    assert!(s.fake.most.load(Ordering::SeqCst) <= 8);
}

#[test]
fn a_loop_measures_and_changes_until_it_is_close() {
    let s = session(10.0);
    let (error, v) = s.call(
        "script",
        json!({"source": r#"
            await studio.snapshot("before level");
            let gain = 0, steps = [];
            for (let i = 0; i < 10; i++) {
              const now = (await studio.run("LISTEN", {track: "Kick"})).figures.lufs;
              const off = args.target - now;
              steps.push(now);
              if (Math.abs(off) <= 0.1) break;
              gain += off * 0.8;
              await studio.trackSet({track: "Kick", volume_db: gain});
            }
            return {gain, steps};
        "#, "args": {"target": -23.0}}),
    );
    assert!(!error, "{v}");
    let steps = v["result"]["steps"].as_array().unwrap();
    let last = steps.last().unwrap().as_f64().unwrap();
    assert!((last + 23.0).abs() <= 0.1, "{v}");
    assert_eq!(s.ran("MIX_SNAPSHOT")[0]["name"], json!("before level"));
}

#[test]
fn a_failed_call_throws_with_its_code_and_can_be_caught() {
    let s = session(10.0);
    let (error, v) = s.script(
        r#"
        try { await studio.run("LISTEN", {track: "Nope"}); }
        catch (e) { return {code: e.code, message: e.message, call: e.call}; }
        "#,
    );
    assert!(!error, "{v}");
    assert_eq!(v["result"]["code"], json!("failed"));
    assert_eq!(v["result"]["call"], json!("run(\"LISTEN\")"));
    // Not caught: the call's failure, the script's line and its log.
    let (error, v) = s.script(
        "log(\"first\");\nconst x = 1;\nawait studio.listen({track: \"Nope\"});\nreturn x;",
    );
    assert!(error);
    assert_eq!(v["code"], json!("failed"), "{v}");
    assert_eq!(v["line"], json!(3), "{v}");
    assert_eq!(
        v["source"],
        json!("await studio.listen({track: \"Nope\"});")
    );
    assert_eq!(v["log"], json!(["first"]));
}

#[test]
fn a_script_error_says_its_line() {
    let s = session(10.0);
    let (error, v) = s.script("const a = 1;\nconst b = a.nope.deeper;\nreturn b;");
    assert!(error);
    assert_eq!(v["code"], json!("script"));
    assert_eq!(v["line"], json!(2), "{v}");
    assert!(v["error"].as_str().unwrap().contains("TypeError"), "{v}");
    let (error, v) = s.script("return (;");
    assert!(error);
    assert!(v["error"].as_str().unwrap().contains("SyntaxError"), "{v}");
    let (error, v) = s.script("throw 'plain';");
    assert!(error);
    assert!(v["error"].as_str().unwrap().contains("plain"), "{v}");
}

#[test]
fn the_sandbox_has_studio_and_nothing_else() {
    let s = session(10.0);
    let (error, v) = s.script(
        r#"return [typeof require, typeof std, typeof os, typeof fetch, typeof setTimeout,
                   typeof process, typeof __send, typeof studio.listen, typeof studio.blockMeasure,
                   typeof studio.script, typeof studio.batch];"#,
    );
    assert!(!error, "{v}");
    assert_eq!(
        v["result"],
        json!([
            "undefined",
            "undefined",
            "undefined",
            "undefined",
            "undefined",
            "undefined",
            "undefined",
            "function",
            "function",
            "undefined",
            "undefined"
        ])
    );
}

#[test]
fn a_runaway_script_is_stopped_at_its_timeout() {
    let s = session(10.0);
    let start = Instant::now();
    let (error, v) = s.call("script", json!({"source": "while (true) {}", "timeout": 1}));
    assert!(error);
    assert_eq!(v["code"], json!("timeout"), "{v}");
    assert!(start.elapsed() < Duration::from_secs(4));
    // A call still running at the timeout is stopped too.
    let (error, v) = s.call(
        "script",
        json!({"source": "await studio.run('LISTEN', {track: 'Kick', slow_ms: 20000});", "timeout": 1}),
    );
    assert!(error);
    assert_eq!(v["code"], json!("timeout"), "{v}");
    assert!(start.elapsed() < Duration::from_secs(8));
}

#[test]
fn a_script_cannot_hold_more_than_its_memory() {
    let s = session(10.0);
    let (error, v) = s.script("const xs = []; while (true) xs.push(new Array(100000).fill(1.5));");
    assert!(error);
    assert!(v["error"].as_str().unwrap().contains("memory"), "{v}");
}

#[test]
fn a_promise_that_never_settles_says_so() {
    let s = session(10.0);
    let (error, v) = s.script("await new Promise(() => {});");
    assert!(error);
    assert!(v["error"].as_str().unwrap().contains("never comes"), "{v}");
}

#[test]
fn a_long_script_becomes_a_job() {
    let s = session(0.1);
    let (_, v) =
        s.script("await studio.run('LISTEN', {track: 'Kick', slow_ms: 600}); return 'done';");
    assert_eq!(v["state"], json!("running"), "{v}");
    let job = v["job"].as_str().unwrap().to_owned();
    let (_, done) = s.call("job", json!({"job": job, "wait": 5}));
    assert_eq!(done["result"], json!("done"), "{done}");
}

#[test]
fn a_cancelled_script_stops_its_calls() {
    let s = session(0.1);
    let (_, v) = s.script(
        "await studio.all([1,2,3].map(() => studio.run('LISTEN', {track: 'K', slow_ms: 20000})));",
    );
    let job = v["job"].as_str().unwrap().to_owned();
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(s.fake.running.load(Ordering::SeqCst), 3);
    s.call("job_cancel", json!({"job": job}));
    let start = Instant::now();
    while s.fake.running.load(Ordering::SeqCst) > 0 {
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "calls still running"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_plain_batch_is_studios_own() {
    let s = session(10.0);
    let (_, v) = s.call("batch", json!({"steps": [{"id": "LISTEN"}]}));
    assert_eq!(v["studio"], json!(true));
}

#[test]
fn a_parallel_batch_runs_read_only_steps_side_by_side() {
    let s = session(10.0);
    let start = Instant::now();
    let steps: Vec<Value> = (0..4)
        .map(|i| json!({"id": "LISTEN", "args": {"track": format!("T{i}"), "slow_ms": 400}}))
        .collect();
    let (error, v) = s.call("batch", json!({"steps": steps, "parallel": true}));
    assert!(!error, "{v}");
    assert_eq!(v["ran"], json!(4));
    assert_eq!(v["results"][3]["result"]["track"], json!("T3"));
    assert!(start.elapsed() < Duration::from_millis(1200));
    let (error, v) = s.call(
        "batch",
        json!({"steps": [{"id": "LISTEN"}, {"id": "TRACK_SET", "args": {"track": "K"}}], "parallel": true}),
    );
    assert!(error);
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("TRACK_SET is mutating"),
        "{v}"
    );
}

#[test]
fn a_batch_step_takes_an_earlier_answer() {
    let s = session(10.0);
    let (error, v) = s.call(
        "batch",
        json!({"steps": [
            {"id": "LISTEN", "args": {"track": "Kick"}},
            {"id": "TRACK_SET", "args": {"track": "Snare", "volume_db": {"$ref": "steps.0.figures.lufs"}}},
        ]}),
    );
    assert!(!error, "{v}");
    assert_eq!(v["results"][1]["result"]["volume_db"], json!(-20.0));
    let (_, v) = s.call(
        "batch",
        json!({"steps": [
            {"id": "LISTEN", "args": {"track": "Kick"}},
            {"id": "TRACK_SET", "args": {"volume_db": {"$ref": "steps.0.figures.nope"}}},
        ]}),
    );
    let e = v["results"][1]["error"]["error"].as_str().unwrap();
    assert!(e.contains("no `nope`") && e.contains("lufs"), "{v}");
    assert_eq!(v["ran"], json!(2));
}
