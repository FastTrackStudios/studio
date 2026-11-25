//! One `script` run against the Studio running on this machine, read-only
//! (`SESSION_TRACKS`, `SESSION_SECTIONS` and `LISTEN` over the first section):
//! skipped when no Studio answers on its control socket.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use studio_mcp::server::Notify;
use studio_mcp::upstream::{Cancel, Socket, Upstream, socket_path};
use studio_mcp::{Bridge, Config};

#[test]
fn a_read_only_script_runs_against_the_live_studio() {
    let env = |k: &str| std::env::var(k).ok();
    let path = socket_path("studio", None, &env);
    let up = Arc::new(Socket::new(path.clone(), Duration::from_secs(1)));
    if !path.exists() || up.call("hello", &json!({}), &Cancel::default()).is_err() {
        // No Studio here (CI, or it is not running): nothing to run against.
        return;
    }
    let notify: Notify = Arc::new(|_| {});
    let bridge = Bridge::new(up, Config::default(), notify);
    let source = r#"
        const sections = await studio.sections();
        const tracks = await studio.run("SESSION_TRACKS");
        const leaf = tracks.find((t) => !t.folder && t.items > 0) ?? tracks[0];
        const s = sections[0];
        const [a, b] = await studio.all([
          studio.listen({track: leaf.path, from: `${s.start}s`, to: `${Math.min(s.end, s.start + 2)}s`}),
          studio.listen({track: leaf.path, from: `${s.start}s`, to: `${Math.min(s.end, s.start + 2)}s`, at: "pre-fx"}),
        ]);
        log("listened to", leaf.path);
        return {tracks: tracks.length, track: leaf.path, lufs: a.figures.lufs, pre_fx_lufs: b.figures.lufs};
    "#;
    let reply = bridge
        .handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": "script", "arguments": {"source": source, "wait": 120}}}))
        .unwrap();
    let text = reply["result"]["content"][0]["text"].as_str().unwrap();
    let v: Value = serde_json::from_str(text).unwrap();
    assert_eq!(reply["result"]["isError"], json!(false), "{v}");
    assert!(v["result"]["tracks"].as_u64().unwrap() > 0, "{v}");
    assert_eq!(v["calls"], json!(4), "{v}");
    assert_eq!(v["log"].as_array().unwrap().len(), 1, "{v}");
}
