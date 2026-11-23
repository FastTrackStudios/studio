//! The app, through the host service.
//!
//! The vault's notes, the engine's actions, the panes: the only way the
//! launcher reaches any of them, so its development copy (a process)
//! behaves as the compiled-in one.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;
use std::sync::{Mutex, PoisonError};
use std::task::{Context, Poll};

use architect_extensions::hostcall::{HostError, HostRequest};
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

/// A runtime for the actions the launcher runs.
///
/// Linked in, an action the launcher runs is polled where the launcher's
/// own future is: a hook's (launch, the deck's moves) is not on Tokio,
/// and an action that needs its reactor (Keyflow's follow opens a socket)
/// panics there, which the same action run from the command line never
/// does. Each call is polled with this runtime entered, so what it
/// spawns and waits on has one. As a process the call crosses to the
/// host and none of this matters. In a browser there is one thread and
/// no Tokio: what an action awaits there is the page's.
#[cfg(not(target_arch = "wasm32"))]
fn runtime() -> Option<&'static tokio::runtime::Runtime> {
    static RUNTIME: OnceLock<Option<tokio::runtime::Runtime>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .thread_name("studio-launcher")
                .enable_all()
                .build()
                .ok()
        })
        .as_ref()
}

/// `future`, each poll of it with [`runtime`] entered.
struct Entered<F>(Pin<Box<F>>);

impl<F: Future> Future for Entered<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        #[cfg(not(target_arch = "wasm32"))]
        let _guard = runtime().map(tokio::runtime::Runtime::enter);
        self.0.as_mut().poll(cx)
    }
}

/// Runs app action `action` with `args`: its answer.
///
/// # Errors
///
/// The launcher is not running, or the action refused.
pub async fn run(action: &str, args: Value) -> Result<Value, String> {
    let call = handle()?.host(HostRequest::Run {
        action: action.to_owned(),
        args,
        ticket: None,
    });
    Entered(Box::pin(call)).await.map_err(|e| e.to_string())
}

/// The service Studio's engine answers (`[uses]` in the manifest): its
/// method `m` is the provider's action `<PREFIX>_<M>`, the `daw`
/// extension's `DAW_<M>` natively and in a browser alike.
pub const SESSION: &str = "fts.session";

/// The action the engine's `fts.session` method `method` is: what is run
/// when the provider the host meets has no such method (Keyflow provides
/// `fts.session` too, for a chart's session, and opens nothing).
fn engine_action(method: &str) -> String {
    format!("DAW_{}", method.to_ascii_uppercase())
}

/// Calls method `method` of Studio's engine (`fts.session`: `open`,
/// `state`, `run`) with `args`, through the host's service.
///
/// The same call natively (the `daw` extension, a process) and in a
/// browser (the engine in the page). When the provider met is not the
/// engine's (it has no such method), the engine's own action is run.
///
/// # Errors
///
/// The launcher is not running, no engine answers, or it refused.
pub async fn session(method: &str, args: Value) -> Result<Value, String> {
    let call = handle()?.service(SESSION, method, args.clone());
    match Entered(Box::pin(call)).await {
        Ok(out) => Ok(out),
        // Another provider of the service (Keyflow's) answered, without it.
        Err(HostError::NotFound { what })
            if what.starts_with(&format!("method {method} of {SESSION}")) =>
        {
            run(&engine_action(method), args).await
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Opens note `path` as the app opens a note the person picks (only
/// what the viewer may open).
///
/// # Errors
///
/// The launcher is not running, or the app refused.
pub async fn open(path: &str) -> Result<Value, String> {
    handle()?
        .host(HostRequest::Open {
            path: path.to_owned(),
        })
        .await
        .map_err(|e| e.to_string())
}

/// The code of a share link of `setlist` (its note's path): a share
/// note (`_architect/shares/<code>.md`) whose `shares` property is the
/// setlist's note (`note:<path>`, as Architect's `share_note` writes it).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub async fn share_of(setlist: &str) -> Option<String> {
    let want = format!("note:{setlist}");
    query(SHARES)
        .await
        .ok()?
        .into_iter()
        .find(|row| row.values.get("shares").and_then(Value::as_str) == Some(want.as_str()))
        .and_then(|row| {
            Path::new(&row.path)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_owned)
        })
}

/// Every share link's note (`_architect/shares/<code>.md`).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const SHARES: &str = "filters:\n  and:\n    - file.folder == \"_architect/shares\"\nviews:\n  - type: table\n    name: Shares\n    order:\n      - file.name\n      - shares\n      - listen\n";

/// Every setlist note.
const SETLISTS: &str = "filters:\n  and:\n    - type == \"setlist\"\nviews:\n  - type: table\n    name: Setlists\n    order:\n      - file.name\n      - title\n      - date\n      - songs\n      - sets\n      - project\n      - session\n      - notes\n      - parts\n      - layout\n      - leads\n      - keys\n      - room\n      - call\n      - starts\n";

/// Every song note, with what says what it is ready with.
const SONGS: &str = "filters:\n  and:\n    - type == \"song\"\nviews:\n  - type: table\n    name: Songs\n    order:\n      - file.name\n      - title\n      - artist\n      - key\n      - key_end\n      - tempo\n      - meter\n      - length\n      - chart\n      - lyrics\n      - media\n      - session\n      - art\n";

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

/// The vault's folder on disk, when it has one (none in a browser).
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
    let out = session("run", json!({ "id": id, "args": args })).await?;
    Ok(match out {
        Value::String(text) => serde_json::from_str(&text).unwrap_or(Value::String(text)),
        other => other,
    })
}

/// The extension's own kept value `key` on this device.
pub async fn data(key: &str) -> Option<Value> {
    handle()
        .ok()?
        .host(HostRequest::Data {
            key: key.to_owned(),
        })
        .await
        .ok()
        .filter(|v| !v.is_null())
}

/// Keeps `value` as `key` on this device (none: forgets it).
///
/// # Errors
///
/// The launcher is not running, or the host refused.
pub async fn set_data(key: &str, value: Option<Value>) -> Result<(), String> {
    handle()?
        .host(HostRequest::SetData {
            key: key.to_owned(),
            value,
        })
        .await
        .map(drop)
        .map_err(|e| e.to_string())
}

/// Sets property `key` of note `path` to `value` (`None`: removes it).
///
/// # Errors
///
/// The vault refused the write.
pub async fn set_property(path: &str, key: &str, value: Option<&Value>) -> Result<(), String> {
    match value {
        // The vault takes a property's value as JSON text, or else as text.
        Some(value) => {
            let value = match value {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            run(
                "VAULT_SET_PROPERTY",
                json!({ "path": path, "key": key, "value": value }),
            )
            .await
        }
        None => run("VAULT_REMOVE_PROPERTY", json!({ "path": path, "key": key })).await,
    }
    .map(drop)
}
