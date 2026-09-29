//! `frame-mcp` — an MCP server (stdio) for designing control surfaces.
//!
//! ```text
//! frame-mcp [--file surface.fm]
//! ```
//! Serves the `design-session` tool registry over MCP. With `--file`, the
//! document is loaded from (and, after every edit, atomically saved to) a
//! frame-markup file — point the `frame` desktop player at the same file
//! and it hot-reloads as the model works.
//!
//! Register with Claude Code:
//! `claude mcp add frame -- /path/to/frame-mcp --file /path/to/surface.fm`

use std::io::{BufRead, Write};
use std::path::PathBuf;

use design_session::{Session, tools};
use serde_json::{Value, json};

const REFERENCE: &str = include_str!("../../../../features/document/document-text/REFERENCE.md");

const INSTRUCTIONS: &str = "frame designs live-performance control surfaces (knobs, faders, footswitches, meters) \
that are bound to an audio engine's parameters. Workflow: `catalog` to see what can be controlled → `describe` \
to see the document → build with `generate` (full starting surface), `add_control` (canonical themed controls), \
or `write` (frame markup; call `reference` for the syntax) → `lint` and fix every error → iterate. Every tool \
call is one undoable step. Bind only to catalog entries (param:<addr>, action:<id>, stream:<id>[:channel]).";

fn main() {
    let mut file: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--file" => file = args.next().map(PathBuf::from),
            "-h" | "--help" => {
                eprintln!("usage: frame-mcp [--file surface.fm]");
                return;
            }
            other => {
                eprintln!("frame-mcp: unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    let catalog = surface_memory::demo_catalog();
    let mut session = match file.as_ref().filter(|p| p.exists()) {
        Some(p) => match std::fs::read_to_string(p).map_err(|e| e.to_string()).and_then(|t| {
            document_text::parse(&t, Default::default()).map_err(|e| e.to_string())
        }) {
            Ok(scene) => Session::new(scene, catalog),
            Err(e) => {
                eprintln!("frame-mcp: can't load {}: {e}", p.display());
                std::process::exit(1);
            }
        },
        None => Session::blank(catalog),
    };

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(&mut session, file.as_ref(), &msg),
            Err(e) => Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": e.to_string()}})),
        };
        if let Some(r) = reply {
            let _ = writeln!(stdout, "{r}");
            let _ = stdout.flush();
        }
    }
}

/// One JSON-RPC message → optional reply (notifications get none).
fn handle(session: &mut Session, file: Option<&PathBuf>, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned()?; // notifications carry no id
    let method = msg.get("method").and_then(Value::as_str).unwrap_or_default();
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params.get("protocolVersion").cloned().unwrap_or(json!("2025-06-18")),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "frame", "version": env!("CARGO_PKG_VERSION")},
            "instructions": INSTRUCTIONS,
        })),
        "ping" => Ok(json!({})),
        "tools/list" => {
            let mut list: Vec<Value> = tools()
                .into_iter()
                .map(|t| json!({"name": t.name, "description": t.description, "inputSchema": t.input_schema}))
                .collect();
            list.push(json!({
                "name": "reference",
                "description": "The frame markup authoring reference (elements, attributes, examples). Read before using `write`/`replace`.",
                "inputSchema": {"type": "object", "properties": {}},
            }));
            Ok(json!({"tools": list}))
        }
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            let (text, is_error) = if name == "reference" {
                (REFERENCE.to_string(), false)
            } else {
                match session.call(name, &args) {
                    Ok(text) => {
                        let mutates = tools().iter().any(|t| t.name == name && t.mutates);
                        match (mutates, file) {
                            (true, Some(path)) => match save(session, path) {
                                Ok(()) => (text, false),
                                Err(e) => (format!("{text}\n(warning: saving {} failed: {e})", path.display()), false),
                            },
                            _ => (text, false),
                        }
                    }
                    Err(e) => (e.0, true),
                }
            };
            Ok(json!({"content": [{"type": "text", "text": text}], "isError": is_error}))
        }
        other => Err(json!({"code": -32601, "message": format!("method not found: {other}")})),
    };
    Some(match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e}),
    })
}

/// Write-then-rename so a watcher never sees a half-written file.
fn save(session: &Session, path: &PathBuf) -> std::io::Result<()> {
    let tmp = path.with_extension("fm.tmp");
    std::fs::write(&tmp, document_text::print(session.scene()))?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rpc(s: &mut Session, method: &str, params: Value) -> Value {
        handle(s, None, &json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})).unwrap()
    }

    #[test]
    fn mcp_handshake_list_and_call() {
        let mut s = Session::blank(surface_memory::demo_catalog());
        let init = rpc(&mut s, "initialize", json!({"protocolVersion": "2025-06-18"}));
        assert_eq!(init["result"]["serverInfo"]["name"], "frame");
        assert!(handle(&mut s, None, &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none());
        let list = rpc(&mut s, "tools/list", json!({}));
        let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"generate") && names.contains(&"reference") && names.contains(&"lint"));
        let r = rpc(&mut s, "tools/call", json!({"name": "generate", "arguments": {}}));
        assert_eq!(r["result"]["isError"], false);
        let r = rpc(&mut s, "tools/call", json!({"name": "bind", "arguments": {"node": "nope"}}));
        assert_eq!(r["result"]["isError"], true);
        assert_eq!(rpc(&mut s, "bogus", json!({}))["error"]["code"], -32601);
    }

    #[test]
    fn edits_save_to_the_file() {
        let dir = std::env::temp_dir().join(format!("frame-mcp-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.fm");
        let mut s = Session::blank(surface_memory::demo_catalog());
        handle(&mut s, Some(&path), &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "generate", "arguments": {}}}));
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(document_text::parse(&text, Default::default()).unwrap(), *s.scene());
        let _ = std::fs::remove_dir_all(dir);
    }
}
