//! `daw`: Studio's engine as an Architect extension.
//!
//! The process runs the engine `studio engine` runs
//! (`session_desktop::engine_host`): one `Standalone`, the set's audio
//! engine, the set, the Action List, and the engine's view service on a
//! Unix socket, published under `$XDG_CONFIG_HOME/fts/engines/` as
//! `studio engine` publishes itself. Studio's views, installed as
//! extensions of their own (`studio.arrangement`, `studio.mixer`), find it
//! there and attach to it exactly as `studio-desktop`'s window attaches to
//! a `studio engine` it did not start (`session_engine::studio_engine::link`):
//! one engine, one window protocol, nothing redrawn or re-modelled.
//!
//! What it opens comes from `FTS_DAW_OPEN` (a song, a `.session` or a
//! setlist, anything `studio engine --open` takes) or the `DAW_OPEN`
//! action.

pub mod deck;

use std::sync::OnceLock;

use architect_extensions::linked::{Builder, Extension};
use serde_json::{Value as Json, json};

pub const MANIFEST: &str = include_str!("../extension.toml");

/// Where the engine said it is, once up.
static STARTED: OnceLock<Result<(), String>> = OnceLock::new();

/// Starts the engine on a thread of its own (it blocks on its runtime
/// until the process ends), opening `open` as it comes up.
pub fn start_engine(open: Option<String>) {
    let args = session_desktop::engine_host::Args {
        open,
        ..session_desktop::engine_host::Args::default()
    };
    let spawned = std::thread::Builder::new()
        .name("studio-engine".to_owned())
        .spawn(move || {
            if let Err(e) = session_desktop::engine_host::run(&args) {
                let _ = STARTED.set(Err(e.to_string()));
            }
        });
    if let Err(e) = spawned {
        let _ = STARTED.set(Err(e.to_string()));
    }
}

/// The engine's state: whether it is up, where, and the song it has.
fn state() -> Json {
    let found = session_engine::studio_engine::link::found()
        .into_iter()
        .find(|f| f.pid == std::process::id());
    json!({
        "up": found.is_some(),
        "socket": found.as_ref().map(|f| f.socket.display().to_string()),
        "mcp": found.as_ref().map(|f| f.mcp.clone()),
        "failed": STARTED.get().and_then(|r| r.as_ref().err().cloned()),
        "deck": {
            "sent": deck::SENT.load(std::sync::atomic::Ordering::Relaxed),
            "followed": deck::FOLLOWED.load(std::sync::atomic::Ordering::Relaxed),
        },
    })
}

/// Runs action `id` of the engine's Action List, awaited: an async one
/// (`SESSION_ZOOM`, the arrangement's gestures) as much as any.
/// `session_engine::actions::run` would refuse those here, where a
/// runtime is current.
async fn run(id: &str, args: &Json) -> Result<Json, String> {
    let out = session_engine::actions::list()
        .run_json_async(id, args.to_string().as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::from_slice(&out)
        .unwrap_or_else(|_| Json::String(String::from_utf8_lossy(&out).into_owned())))
}

pub struct Daw;

impl Extension for Daw {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        deck::set_handle(ext.handle());
        ext.action("DAW_STATE", |_| async { Ok(state()) })
            .action("DAW_OPEN", |args| async move {
                let path = args.get("path").and_then(Json::as_str).ok_or("no path")?;
                run("SESSION_OPEN", &json!({ "path": path })).await
            })
            .action("DAW_DECK", |args| async move {
                // The app's playhead moved (its hook): the engine follows
                // a move that was not its own.
                deck::heard(&args);
                Ok(json!({ "ok": true }))
            })
            .action("DAW_RUN", |args| async move {
                let id = args.get("id").and_then(Json::as_str).ok_or("no id")?;
                run(id, args.get("args").unwrap_or(&Json::Null)).await
            });
    }
}
