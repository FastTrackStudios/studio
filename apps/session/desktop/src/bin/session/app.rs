//! `session app` — drive the Session window that is running, through its
//! Action List over MCP (`session_daw::mcp_server`).
//!
//! The window says where it listens in `$XDG_CONFIG_HOME/fts/session-mcp.json`
//! (`~/.config/fts/…`); `FTS_MCP_URL` names another. `session app actions`
//! lists what the window can do, `session app run <ACTION> [json]` runs
//! one, and `open` / `open-setlist` are the two a test run starts with.

use std::time::Duration;

use serde_json::{Value, json};

/// Where the running window listens.
fn url() -> eyre::Result<String> {
    if let Ok(url) = std::env::var("FTS_MCP_URL")
        && !url.is_empty()
    {
        return Ok(url);
    }
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".config")))
        .ok_or_else(|| eyre::eyre!("no config directory"))?;
    let file = config.join("fts").join("session-mcp.json");
    let text = std::fs::read_to_string(&file).map_err(|e| {
        eyre::eyre!(
            "no running Session window found ({}: {e}) — start one, or set FTS_MCP_URL",
            file.display()
        )
    })?;
    let found: Value = serde_json::from_str(&text)?;
    found
        .get("url")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| eyre::eyre!("{}: no url", file.display()))
}

/// One JSON-RPC call to the window; its `result`.
fn call(url: &str, method: &str, params: Value) -> eyre::Result<Value> {
    let token = std::env::var("FTS_MCP_TOKEN")
        .ok()
        .filter(|t| !t.is_empty());
    let mut request = ureq::post(url)
        .timeout(Duration::from_secs(60))
        .set("content-type", "application/json")
        .set("accept", "application/json, text/event-stream");
    if let Some(token) = token {
        request = request.set("authorization", &format!("Bearer {token}"));
    }
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let answer: Value = serde_json::from_str(
        &request
            .send_string(&body.to_string())
            .map_err(|e| eyre::eyre!("{url}: {e}"))?
            .into_string()?,
    )?;
    if let Some(error) = answer.get("error") {
        eyre::bail!("{method}: {error}");
    }
    Ok(answer.get("result").cloned().unwrap_or(Value::Null))
}

/// The text of a tool's result; an error when the tool says it failed.
fn said(tool: &str, result: &Value) -> eyre::Result<String> {
    let text: Vec<&str> = result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.get("text").and_then(Value::as_str))
        .collect();
    let said = text.join("\n");
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        eyre::bail!("{tool}: {said}");
    }
    Ok(said)
}

/// The ids `action` may be, in the order to try them: as written (upper
/// case, `-` as `_`), then under Session's own prefix — so `open`,
/// `open-setlist` and `SESSION_OPEN` all work, and so does an action of
/// another prefix (`TRANSPORT_PLAY`).
fn action_ids(action: &str) -> Vec<String> {
    let id = action.trim().to_uppercase().replace('-', "_");
    if id.starts_with("SESSION_") {
        vec![id]
    } else {
        vec![id.clone(), format!("SESSION_{id}")]
    }
}

/// Run `action` with `args` in the running window (its Action List's
/// `run` tool); what it said.
pub fn run(action: &str, args: Value) -> eyre::Result<String> {
    let url = url()?;
    let mut last = None;
    for id in action_ids(action) {
        let mut params = json!({"id": id});
        if !args.is_null() {
            params["args"] = args.clone();
        }
        let result = call(
            &url,
            "tools/call",
            json!({"name": "run", "arguments": params}),
        )?;
        match said(&id, &result) {
            Ok(text) => return Ok(text),
            // Not an action of this window: the next spelling may be.
            Err(e)
                if e.to_string().contains("no such action")
                    || e.to_string().contains("no action ") =>
            {
                last = Some(e)
            }
            Err(e) => return Err(e),
        }
    }
    Err(last
        .unwrap_or_else(|| eyre::eyre!("no action {action:?} — `session app actions` lists them")))
}

/// The running window's actions (its Action List's `actions` tool),
/// filtered by `filter` when given.
pub fn list(filter: Option<&str>) -> eyre::Result<()> {
    let url = url()?;
    let args = filter.map_or_else(|| json!({}), |f| json!({"filter": f}));
    let result = call(
        &url,
        "tools/call",
        json!({"name": "actions", "arguments": args}),
    )?;
    println!("{}", said("actions", &result)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::action_ids;

    #[test]
    fn an_action_is_tried_as_written_then_under_sessions_prefix() {
        assert_eq!(action_ids("open"), ["OPEN", "SESSION_OPEN"]);
        assert_eq!(
            action_ids("open-setlist"),
            ["OPEN_SETLIST", "SESSION_OPEN_SETLIST"]
        );
        assert_eq!(action_ids("SESSION_OPEN"), ["SESSION_OPEN"]);
        assert_eq!(
            action_ids("transport_play"),
            ["TRANSPORT_PLAY", "SESSION_TRANSPORT_PLAY"]
        );
    }
}
