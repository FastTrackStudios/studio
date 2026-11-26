//! The engine's MCP door, as an agent reaches it: JSON-RPC over HTTP on
//! loopback, every action through the `run` tool. Nothing of the engine's
//! code is linked: only its protocol.

use std::io::{Read as _, Write as _};
use std::net::TcpStream;
use std::time::Duration;

use serde_json::{Value, json};

/// How long one call may take: a whole-song render is minutes on a slow
/// day.
const CALL_WAIT: Duration = Duration::from_secs(30 * 60);

/// A client of one engine's door.
#[derive(Clone, Debug)]
pub struct Mcp {
    /// `host:port`.
    at: String,
    /// The request path (`/mcp`).
    path: String,
}

impl Mcp {
    /// The door at `url` (`http://127.0.0.1:43919/mcp`).
    ///
    /// # Errors
    ///
    /// `url` is not an `http://host:port/path` URL.
    pub fn new(url: &str) -> Result<Self, String> {
        let rest = url
            .strip_prefix("http://")
            .ok_or_else(|| format!("not an http URL: {url}"))?;
        let (at, path) = rest.split_once('/').unwrap_or((rest, "mcp"));
        Ok(Self {
            at: at.to_owned(),
            path: format!("/{path}"),
        })
    }

    /// Whether the door answers.
    #[must_use]
    pub fn ping(&self) -> bool {
        self.rpc("ping", &json!({})).is_ok()
    }

    /// Run action `id` with `args`: its answer, or its error as the
    /// engine said it.
    ///
    /// # Errors
    ///
    /// The action failed, or the door could not be reached.
    pub fn run(&self, id: &str, args: &Value) -> Result<Value, String> {
        let result = self.rpc(
            "tools/call",
            &json!({"name": "run", "arguments": {"id": id, "args": args}}),
        )?;
        let text = result["content"][0]["text"].as_str().unwrap_or_default();
        if result["isError"].as_bool() == Some(true) {
            return Err(format!("{id}: {text}"));
        }
        if let Some(structured) = result.get("structuredContent") {
            return Ok(structured.clone());
        }
        Ok(serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned())))
    }

    /// One JSON-RPC call: its `result`.
    fn rpc(&self, method: &str, params: &Value) -> Result<Value, String> {
        let body =
            json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
        let mut stream = TcpStream::connect(&self.at).map_err(|e| format!("{}: {e}", self.at))?;
        stream
            .set_read_timeout(Some(CALL_WAIT))
            .map_err(|e| e.to_string())?;
        let head = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nAccept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.path,
            self.at,
            body.len()
        );
        stream
            .write_all(head.as_bytes())
            .and_then(|()| stream.write_all(body.as_bytes()))
            .map_err(|e| format!("{method}: {e}"))?;
        let mut reply = Vec::new();
        stream
            .read_to_end(&mut reply)
            .map_err(|e| format!("{method}: {e}"))?;
        let split = reply
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or_else(|| format!("{method}: no HTTP reply"))?;
        let status = String::from_utf8_lossy(&reply[..split]);
        if !status.starts_with("HTTP/1.1 200") {
            return Err(format!(
                "{method}: {}",
                status.lines().next().unwrap_or_default()
            ));
        }
        let answer: Value = serde_json::from_slice(&reply[split + 4..])
            .map_err(|e| format!("{method}: the reply is not JSON: {e}"))?;
        if let Some(error) = answer.get("error") {
            return Err(format!("{method}: {error}"));
        }
        Ok(answer["result"].clone())
    }
}
