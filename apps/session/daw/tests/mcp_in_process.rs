//! Session's MCP face, in process: the same `architect_mcp::Server` that
//! `mcp_server::start` serves on `127.0.0.1:47811/mcp`, over the window's
//! Action List, fed JSON-RPC messages directly with no socket. It names
//! architect's five tools, and SESSION_TRACKS answers through `run` with the
//! open song's tracks, as a model connected to the window would see them.
//! Then the same over its HTTP face (`architect_mcp::http::serve`, what the
//! window listens with), on a free loopback port.
//!
//! One test, one process: the daw facade is bound to the first engine
//! opened.
#![cfg(feature = "native")]

use serde_json::{Value, json};
use session_daw::actions::list;

/// One JSON-RPC request in, its `result` out.
fn call(server: &architect_mcp::Server, id: u64, method: &str, params: &Value) -> Value {
    let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
    let reply = server
        .handle(&request.to_string())
        .unwrap_or_else(|| panic!("{method}: a request with an id gets a reply"));
    let reply: Value = serde_json::from_str(&reply).expect("the reply is JSON");
    assert_eq!(reply["id"], id, "{method}: the reply answers this request");
    assert!(reply.get("error").is_none(), "{method} failed: {reply}");
    reply["result"].clone()
}

/// The `run` tool's answer to an action, parsed from its text content.
fn run_tool(server: &architect_mcp::Server, id: u64, action: &str) -> Value {
    let result = call(
        server,
        id,
        "tools/call",
        &json!({"name": "run", "arguments": {"id": action, "args": {}}}),
    );
    assert_eq!(result["isError"], false, "{action}: {result}");
    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("{action}: text content in {result}"));
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{action} said {text}: {e}"))
}

#[test]
fn the_mcp_server_lists_five_tools_and_runs_session_tracks() {
    let server = architect_mcp::Server::new(
        "FastTrackStudio Session",
        env!("CARGO_PKG_VERSION"),
        list().clone(),
    );

    let init = call(&server, 1, "initialize", &json!({}));
    assert_eq!(init["serverInfo"]["name"], "FastTrackStudio Session");

    let tools = call(&server, 2, "tools/list", &json!({}));
    let names: Vec<&str> = tools["tools"]
        .as_array()
        .expect("a list of tools")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert_eq!(names, ["actions", "describe", "run", "batch", "watch"]);

    // A song to read: a new one holds only the guide.
    run_tool(&server, 3, "SESSION_NEW");
    let rows = run_tool(&server, 4, "SESSION_TRACKS");
    let paths: Vec<&str> = rows
        .as_array()
        .expect("SESSION_TRACKS gives a list of tracks")
        .iter()
        .filter_map(|r| r["path"].as_str())
        .collect();
    for path in ["Guide", "Guide/Click", "Guide/Count", "Guide/Guide"] {
        assert!(paths.contains(&path), "no {path} among {paths:?}");
    }

    // The same server on its HTTP face, as `mcp_server::start` serves it
    // (there on 127.0.0.1:47811, here on a free loopback port).
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bound");
        let at = listener.local_addr().expect("an address");
        tokio::spawn(architect_mcp::http::serve(listener, server, None));

        let listed = post(
            at,
            &json!({"jsonrpc": "2.0", "id": 5, "method": "tools/list"}),
        )
        .await;
        let names: Vec<&str> = listed["result"]["tools"]
            .as_array()
            .expect("a list of tools over HTTP")
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert_eq!(names, ["actions", "describe", "run", "batch", "watch"]);

        let ran = post(
            at,
            &json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call",
                    "params": {"name": "run", "arguments": {"id": "SESSION_TRACKS"}}}),
        )
        .await;
        assert_eq!(ran["result"]["isError"], false, "{ran}");
        let text = ran["result"]["content"][0]["text"].as_str().expect("text");
        assert!(
            text.contains("Guide/Click"),
            "SESSION_TRACKS over HTTP: {text}"
        );
    });
}

/// POST one JSON-RPC message to `/mcp` and parse the JSON reply.
async fn post(at: std::net::SocketAddr, message: &Value) -> Value {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let body = message.to_string();
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: {at}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut stream = tokio::net::TcpStream::connect(at).await.expect("connected");
    stream.write_all(request.as_bytes()).await.expect("sent");
    let mut reply = String::new();
    stream.read_to_string(&mut reply).await.expect("read");
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    let (_, json) = reply.split_once("\r\n\r\n").expect("a body");
    serde_json::from_str(json).unwrap_or_else(|e| panic!("{json}: {e}"))
}
