//! The app, through the host service: the vault's notes, the engine's
//! actions, the panes. The only way the launcher reaches any of them, so
//! its development copy (a process) behaves as the compiled-in one.

use std::sync::{Mutex, PoisonError};

use architect_extensions::hostcall::HostRequest;
use architect_extensions::linked::Handle;
use serde_json::{Value, json};

use crate::model::{Row, rows_of};

static HANDLE: Mutex<Option<Handle>> = Mutex::new(None);

pub fn connect(handle: Handle) {
    *HANDLE.lock().unwrap_or_else(PoisonError::into_inner) = Some(handle);
}

fn handle() -> Result<Handle, String> {
    HANDLE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .ok_or_else(|| "the launcher is not running".to_owned())
}

/// Runs app action `action` with `args`: its answer.
///
/// # Errors
///
/// The launcher is not running, or the action refused.
pub async fn run(action: &str, args: Value) -> Result<Value, String> {
    handle()?
        .host(HostRequest::Run {
            action: action.to_owned(),
            args,
            ticket: None,
        })
        .await
        .map_err(|e| e.to_string())
}

/// Every setlist note.
const SETLISTS: &str = "filters:\n  and:\n    - type == \"setlist\"\nviews:\n  - type: table\n    name: Setlists\n    order:\n      - file.name\n      - title\n      - date\n      - songs\n      - sets\n      - project\n      - session\n";

/// Every song note, with what says what it is ready with.
const SONGS: &str = "filters:\n  and:\n    - type == \"song\"\nviews:\n  - type: table\n    name: Songs\n    order:\n      - file.name\n      - title\n      - artist\n      - key\n      - key_end\n      - tempo\n      - meter\n      - length\n      - chart\n      - lyrics\n      - media\n      - session\n";

async fn query(source: &str) -> Result<Vec<Row>, String> {
    let out = handle()?
        .host(HostRequest::QueryVault {
            path: None,
            source: Some(source.to_owned()),
            view: None,
            if_changed: None,
            limit: Some(5000),
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows_of(&out))
}

/// The vault's setlist rows and song rows.
///
/// # Errors
///
/// The vault's index refused the query.
pub async fn notes() -> Result<(Vec<Row>, Vec<Row>), String> {
    Ok((query(SETLISTS).await?, query(SONGS).await?))
}

/// The vault's folder on disk, when it has one.
pub async fn vault_root() -> Option<String> {
    let out = run("VAULT_STATUS", json!({})).await.ok()?;
    out.get("root")
        .and_then(Value::as_str)
        .map(|r| r.trim_end_matches('/').to_owned())
}

/// An engine action's answer (`DAW_RUN`), a text of JSON read as JSON.
///
/// # Errors
///
/// The engine is not running, or the action refused.
pub async fn engine(id: &str, args: Value) -> Result<Value, String> {
    let out = run("DAW_RUN", json!({ "id": id, "args": args })).await?;
    Ok(match out {
        Value::String(text) => serde_json::from_str(&text).unwrap_or(Value::String(text)),
        other => other,
    })
}
