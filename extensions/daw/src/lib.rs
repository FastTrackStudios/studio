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
//!
//! **In a browser** (studio-web, `wasm32`) the extension is linked into
//! the web app and the engine is in the page ([`web`]): the same id, the
//! same actions where a page has them, session-engine's web engine
//! (`session_engine::web_engine`) hosted by `session_ui::web_host`, and
//! the engine given to the app as the playhead's clock
//! (`architect_playhead::engine`).

#[cfg(not(target_arch = "wasm32"))]
pub mod buttons;
#[cfg(not(target_arch = "wasm32"))]
pub mod deck;
#[cfg(not(target_arch = "wasm32"))]
pub mod meters;
#[cfg(not(target_arch = "wasm32"))]
pub mod owner;
#[cfg(not(target_arch = "wasm32"))]
pub mod session_doc;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;

#[cfg(not(target_arch = "wasm32"))]
use architect_extensions::linked::{Builder, Extension};
#[cfg(not(target_arch = "wasm32"))]
use serde_json::{Value as Json, json};

pub const MANIFEST: &str = include_str!("../extension.toml");

#[cfg(target_arch = "wasm32")]
pub mod web;
#[cfg(target_arch = "wasm32")]
pub use web::Daw;

/// Where the engine said it is, once up.
#[cfg(not(target_arch = "wasm32"))]
static STARTED: OnceLock<Result<(), String>> = OnceLock::new();

/// Starts the engine on a thread of its own (it blocks on its runtime
/// until the process ends), opening `open` as it comes up.
/// What the engine was asked to open at start (`FTS_DAW_OPEN`).
#[cfg(not(target_arch = "wasm32"))]
static OPENED: OnceLock<Option<String>> = OnceLock::new();

/// What the engine was asked to open at start, when anything.
#[must_use]
#[cfg(not(target_arch = "wasm32"))]
pub fn opened() -> Option<&'static str> {
    OPENED.get().and_then(Option::as_deref)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn start_engine(open: Option<String>) {
    let _ = OPENED.set(open.clone());
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
#[cfg(not(target_arch = "wasm32"))]
fn state() -> Json {
    let found = session_engine::studio_engine::link::found()
        .into_iter()
        .find(|f| f.pid == std::process::id());
    json!({
        "up": found.is_some(),
        "socket": found.as_ref().map(|f| f.socket.display().to_string()),
        "mcp": found.as_ref().map(|f| f.mcp.clone()),
        "failed": STARTED.get().and_then(|r| r.as_ref().err().cloned()),
        "session": session_doc::state(),
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
#[cfg(not(target_arch = "wasm32"))]
async fn run(id: &str, args: &Json) -> Result<Json, String> {
    let out = session_engine::actions::list()
        .run_json_async(id, args.to_string().as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::from_slice(&out)
        .unwrap_or_else(|_| Json::String(String::from_utf8_lossy(&out).into_owned())))
}

/// The engine project of the set's song `song` names: its number in the
/// set (from 1), its title, or its project's guid.
#[cfg(not(target_arch = "wasm32"))]
fn song_project(song: &Json) -> Result<String, String> {
    let lineup = session_engine::studio_engine::set::lineup().ok_or("no set is open")?;
    let found = match song {
        Json::Number(n) => n
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .and_then(|n| n.checked_sub(1))
            .and_then(|i| lineup.songs.get(i)),
        Json::String(text) => lineup
            .songs
            .iter()
            .find(|(title, project)| title.eq_ignore_ascii_case(text.trim()) || project == text),
        _ => None,
    };
    found
        .map(|(_, project)| project.clone())
        .ok_or_else(|| format!("no song {song} in the set (DAW_SONGS lists them)"))
}

/// Action `id` of the engine's list, run on song `project` of the set
/// without going to it (`session_engine::open::in_song`).
#[cfg(not(target_arch = "wasm32"))]
fn run_in(project: &str, id: &str, args: &Json) -> Result<Json, String> {
    let list = session_engine::actions::list();
    let bytes = args.to_string();
    let out = session_engine::open::in_song(project, || {
        match list.run_json(id, bytes.as_bytes()) {
            // An async action: awaited on this thread, so the song it is
            // scoped to holds for all of it.
            Err(e) if e.to_string().contains("run_json_async") => {
                let handle = tokio::runtime::Handle::try_current().map_err(|e| e.to_string())?;
                tokio::task::block_in_place(|| {
                    handle
                        .block_on(list.run_json_async(id, bytes.as_bytes()))
                        .map_err(|e| e.to_string())
                })
            }
            other => other.map_err(|e| e.to_string()),
        }
    })?;
    Ok(serde_json::from_slice(&out)
        .unwrap_or_else(|_| Json::String(String::from_utf8_lossy(&out).into_owned())))
}

/// The outputs this machine has, as the engine could open them.
#[cfg(not(target_arch = "wasm32"))]
fn output_devices() -> Vec<Json> {
    audio::io::output_devices(&audio::io::audio_host())
        .into_iter()
        .map(|d| {
            json!({
                "name": d.name,
                "channels": d.channels,
                "sample_rate": d.default_sample_rate,
            })
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
pub struct Daw;

#[cfg(not(target_arch = "wasm32"))]
impl Extension for Daw {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        deck::set_handle(ext.handle());
        ext.action_in("DAW_READ", |args, cx| async move {
            session_doc::call(fts_session::service::READ, &args, &cx).await
        })
        .action_in("DAW_WRITE", |args, cx| async move {
            session_doc::call(fts_session::service::WRITE, &args, &cx).await
        })
        .action_in("DAW_STRUCTURE", |args, cx| async move {
            session_doc::call(fts_session::service::STRUCTURE, &args, &cx).await
        })
        .action("DAW_STATE", |_| async { Ok(state()) })
        .action("DAW_OPEN", |args| async move {
            let path = args.get("path").and_then(Json::as_str).ok_or("no path")?;
            run("SESSION_OPEN", &json!({ "path": path })).await
        })
        .action("DAW_AUDIO", |args| async move {
            // With nothing to change, the device as kept: no reopening.
            let change = args.as_object().is_some_and(|a| !a.is_empty());
            let device = if change {
                run("SESSION_AUDIO_DEVICE", &args).await?
            } else {
                run("SESSION_AUDIO_DEVICE", &json!({})).await?
            };
            Ok(json!({ "device": device, "devices": output_devices() }))
        })
        .action("DAW_DECK", |args| async move {
            // The app's playhead moved (its hook): the engine follows
            // a move that was not its own.
            deck::heard(&args);
            Ok(json!({ "ok": true }))
        })
        .action("DAW_RUN", |args| async move {
            let id = args.get("id").and_then(Json::as_str).ok_or("no id")?;
            let none = Json::Null;
            let with = args.get("args").unwrap_or(&none);
            match args.get("song").filter(|s| !s.is_null()) {
                None => run(id, with).await,
                Some(song) => run_in(&song_project(song)?, id, with),
            }
        })
        .action("DAW_SONGS", |_| async {
            let lineup = session_engine::studio_engine::set::lineup();
            Ok(json!({
                "at": lineup.as_ref().map(|l| l.at + 1),
                "songs": lineup.map(|l| l.songs.iter().enumerate().map(|(i, (title, project))| {
                    json!({ "number": i + 1, "title": title, "project": project })
                }).collect::<Vec<_>>()),
            }))
        });
    }
}
