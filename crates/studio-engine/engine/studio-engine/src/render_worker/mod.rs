//! Agent renders in a process of their own, so a plugin that crashes in
//! one takes down a render worker, never the person's session.
//!
//! `LISTEN`, `LISTEN_BLOCK`, `LISTEN_AB`, `LISTEN_RENDER`, `MIX_MEASURE`
//! and `MIX_RENDER` render offline on copies of the song's plugins. Those
//! copies are in-process CLAP instances: a segfault in one, during the
//! hundredth render an agent asks for, kills the engine and the song
//! with it (`docs/design/opendaw-lynx-comparison.md`, item 7).
//!
//! # Where a render runs
//!
//! The render actions are registered on a list of this module's
//! ([`register_routed`]), and the engine's list holds a forward of each
//! that decides, per call, where it runs ([`policy::decide`]): in process,
//! as before, or in the **render worker**. The default keeps every render
//! in process, on the song the engine already holds; `auto` sends an
//! agent's renders to the worker (crash isolation, at the cost of loading
//! the song a second time), set by the `RENDER_WORKER` action or
//! `FTS_RENDER_WORKER`. An agent's
//! call is told apart by a principal on its caller (the engine's MCP
//! door), an `Agent` invocation, or `"_origin": "agent"` in its arguments
//! (what survives every hop from Studio's control socket to here).
//!
//! # The worker
//!
//! The engine binary in worker mode (`daw-extension --render-worker`,
//! `serve`): spawned on the first render routed to it, kept warm,
//! replaced when it dies (`client`). It holds the song as the live engine
//! does: built from a Loro snapshot of the live song's doc, then kept in
//! step by the doc's updates since the version it last reported, applied
//! by the bridge as another device's edits are. Plugin states come from
//! the live song's project file as the worker asks for them. So its plugin
//! instances, warm copies and render cache stay across calls, and an
//! unchanged song costs no sync at all.
//!
//! A worker that dies mid-call answers that call `plugin_crashed`, naming
//! the plugin when its crash report can (`crash`); a worker that hangs past
//! the timeout answers `worker_timeout`. Either way a fresh one starts.

pub mod client;
pub mod crash;
pub mod policy;
pub mod serve;
pub mod wire;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::{Arc, PoisonError};

use architect::action::{
    ActionCaller, ActionError, ActionFuture, ActionList, ActionMeta, CallerAsyncHandler, Effect,
    RawJson,
};
use facet::Facet;
use serde_json::json;

use policy::{Mode, Where};

pub use serve::main_if_worker;

/// The list the routed actions really live on.
fn local() -> &'static ActionList {
    static LOCAL: std::sync::LazyLock<ActionList> = std::sync::LazyLock::new(ActionList::new);
    &LOCAL
}

/// Registers what `register` registers onto `list`, each action run where
/// [`route`] says: the one hook between the render actions and the worker.
///
/// # Errors
/// As `register`, or an id already on `list`.
pub fn register_routed(
    list: &ActionList,
    register: fn(&ActionList) -> Result<(), ActionError>,
) -> Result<(), ActionError> {
    let inner = local();
    let before: std::collections::HashSet<String> =
        inner.infos().into_iter().map(|i| i.id).collect();
    register(inner)?;
    for info in inner.infos() {
        if before.contains(&info.id) {
            continue;
        }
        let id = info.id.clone();
        let handler: CallerAsyncHandler = Arc::new(move |caller: &ActionCaller, raw: &[u8]| {
            let (id, caller) = (id.clone(), caller.clone());
            let raw = raw.to_vec();
            // The song the call is scoped to (`DAW_RUN` with a `song`) is
            // this thread's: carried to the thread the work runs on.
            let scoped = crate::open::current_song();
            let run: ActionFuture = Box::pin(async move {
                let failed = |message: String| ActionError::Failed {
                    id: id.clone(),
                    message,
                };
                let owned = id.clone();
                crate::off_loop::blocking(move || match scoped {
                    Some(song) => crate::open::in_song(&song, || route(&caller, &owned, &raw)),
                    None => route(&caller, &owned, &raw),
                })
                .await
                .map_err(failed)?
            });
            Ok(run)
        });
        let mut info = info;
        info.params_schema = info.params_schema.as_deref().map(with_routing_words);
        list.register_forwarded_with(&info, handler)?;
    }
    Ok(())
}

/// `schema` (an action's JSON Schema) with [`policy::ORIGIN_ARG`] and
/// [`policy::WHERE_ARG`] among its properties, so a call may carry them.
fn with_routing_words(schema: &str) -> String {
    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(schema) else {
        return schema.to_owned();
    };
    if let Some(props) = v
        .as_object_mut()
        .map(|o| o.entry("properties").or_insert_with(|| json!({})))
        .and_then(serde_json::Value::as_object_mut)
    {
        props.insert(
            policy::ORIGIN_ARG.to_owned(),
            json!({ "type": "string", "enum": ["agent", "app"],
                    "description": "Who the call is from: an agent's render runs in the render worker (RENDER_WORKER)." }),
        );
        props.insert(
            policy::WHERE_ARG.to_owned(),
            json!({ "type": "string", "enum": ["worker", "process"],
                    "description": "Where this call renders, whatever RENDER_WORKER's mode says." }),
        );
    }
    v.to_string()
}

/// Runs routed action `id` for `caller` where the policy says.
fn route(caller: &ActionCaller, id: &str, raw: &[u8]) -> Result<Vec<u8>, ActionError> {
    let routed = policy::strip(raw);
    let origin = routed.origin.unwrap_or_else(|| policy::origin_of(caller));
    let here = if serve::IS_WORKER.load(std::sync::atomic::Ordering::SeqCst) {
        Where::Process
    } else {
        policy::decide(policy::mode(), origin, id, routed.asked, &routed.args)
    };
    crate::telemetry::set("render.where", here.name());
    crate::telemetry::set("render.origin", origin.name());
    if here == Where::Process {
        return local().run_json_with(caller, id, &routed.args);
    }
    let song = match LiveSong::current() {
        Ok(song) => song,
        Err(why) => {
            // A song with no project file cannot be handed over: it renders
            // here, as it did before there was a worker.
            crate::telemetry::set("render.worker_skipped", why);
            return local().run_json_with(caller, id, &routed.args);
        }
    };
    client::global()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .run(&song, id, &routed.args)
        .map_err(|f| ActionError::Failed {
            id: id.to_owned(),
            message: f.message(),
        })
}

/// The song open in this engine, as the worker is synced from it: its doc
/// as the project file's bridge holds it, its blobs from the file.
pub struct LiveSong {
    guid: String,
    file: PathBuf,
}

impl LiveSong {
    /// The song the call is about (the scoped one, else the current).
    ///
    /// # Errors
    /// No song is open, or it is not open from a project file.
    pub fn current() -> Result<Self, String> {
        let guid = crate::open::current_song().ok_or("no song is open")?;
        let opened = crate::project_file::path_of(&guid)
            .ok_or("the song is not open from a project file (a .session)")?;
        let (file, _) = crate::project_file::file_and_branch(&opened)
            .ok_or("the song is not open from a project file (a .session)")?;
        Ok(Self { guid, file })
    }
}

impl client::Song for LiveSong {
    fn key(&self) -> String {
        self.guid.clone()
    }

    fn name(&self) -> String {
        self.file
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
    }

    fn media_dir(&self) -> PathBuf {
        self.file.parent().map(PathBuf::from).unwrap_or_default()
    }

    fn snapshot(&self) -> Result<(Vec<u8>, Vec<u8>), String> {
        crate::project_file::with_song_doc(&self.guid, |doc| {
            let bytes = doc
                .export(engine_sync::loro::ExportMode::Snapshot)
                .map_err(|e| e.to_string())?;
            Ok((bytes, doc.oplog_vv().encode()))
        })?
    }

    fn updates_since(&self, vv: &[u8]) -> Result<Option<(Vec<u8>, Vec<u8>)>, String> {
        let from = engine_sync::loro::VersionVector::decode(vv).map_err(|e| e.to_string())?;
        crate::project_file::with_song_doc(&self.guid, |doc| {
            let now = doc.oplog_vv();
            if now == from {
                return Ok(None);
            }
            let bytes = doc
                .export(engine_sync::loro::ExportMode::updates(&from))
                .map_err(|e| e.to_string())?;
            Ok(Some((bytes, now.encode())))
        })?
    }

    fn blob(&self, name: &str) -> Option<Vec<u8>> {
        crate::project_file::with_open_store(&self.guid, |store, _| {
            store.blob(name).map_err(|e| e.to_string())
        })
        .ok()
    }
}

static WORKER: ActionMeta = crate::actions::meta(
    "RENDER_WORKER",
    "render_worker",
    "Render worker",
    "Where an agent's offline renders run (LISTEN, LISTEN_BLOCK, LISTEN_AB, LISTEN_RENDER, MIX_MEASURE, MIX_RENDER): in the render worker, a process of its own whose crash never reaches the live session, or in this engine. `mode`: `process` (the default: all of them here, on the song this engine already holds), `auto` (an agent's renders in the worker, the app's own here), `worker` (all of them there); kept across restarts, unless `FTS_RENDER_WORKER` overrides it (then `overridden: true`). `restart: true` starts a fresh worker. A single call can say where it runs with `\"_render_in\": \"worker\"|\"process\"`, and who it is from with `\"_origin\": \"agent\"|\"app\"`, among its own arguments. A render that the worker dies in answers `plugin_crashed: …` naming the plugin it was processing when known; one that hangs past `FTS_RENDER_WORKER_TIMEOUT_S` (1800) answers `worker_timeout`; a fresh worker starts behind either. Answers `mode`, `kept`, `overridden`, and the worker: `pid` (null when none runs), `song` (the one it holds), `spawns`, `restarts`, `renders`, `last_crash`, `last_spawn_ms`, `last_sync_ms`.",
    "Mix",
    "",
);

/// RENDER_WORKER's arguments.
#[derive(Facet)]
struct WorkerArg {
    mode: Option<String>,
    restart: Option<bool>,
}

/// The `RENDER_WORKER` action.
///
/// # Errors
/// The id is already on `list`.
pub fn register(list: &ActionList) -> Result<(), ActionError> {
    list.register_with(
        &WORKER,
        Effect::Mutating,
        |_: &ActionCaller, a: WorkerArg| {
            if let Some(text) = &a.mode {
                let mode = Mode::parse(text)
                    .ok_or_else(|| format!("`mode` is auto, worker or process, not {text:?}"))?;
                policy::keep(mode)?;
            }
            if a.restart == Some(true) {
                let mut worker = client::global()
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                worker.stop();
                worker.ensure().map_err(|f| f.message())?;
            }
            let s = client::status();
            Ok::<_, String>(RawJson::from(
                json!({
                    "mode": policy::mode().name(),
                    "kept": policy::kept().map(Mode::name),
                    "overridden": policy::overridden(),
                    "pid": s.pid,
                    "song": s.song,
                    "spawns": s.spawns,
                    "restarts": s.restarts,
                    "renders": s.renders,
                    "last_crash": s.last_crash,
                    "last_spawn_ms": s.last_spawn_ms,
                    "last_sync_ms": s.last_sync_ms,
                })
                .to_string(),
            ))
        },
    )
}
