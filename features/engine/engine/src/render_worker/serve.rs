//! The render worker's side: the engine binary in worker mode.
//!
//! It starts the same engine the person's runs (silent, no MCP, no
//! autosave, no collaboration), connects back to the engine that spawned
//! it and answers its requests one at a time (`super::wire`). It holds one
//! song: built once from a Loro snapshot of the live song's doc, then kept
//! in step with the doc's updates through the same bridge that applies
//! another device's edits (`Bridge::remote_changed`). Its plugin instances,
//! warm copies and render cache live as long as it does, so a repeat
//! render costs here what it costs in the live engine.
//!
//! A plugin's state is a blob the doc names by hash. The worker has no
//! project file to read them from: it asks the engine for each one it does
//! not hold, as the bridge needs it (`{"need": "blob"}`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Value, json};
use session::sync::engine::{Blobs, MediaRoot};
use session::sync::loro::LoroDoc;
use session::sync::{Bridge, SessionDoc};

use super::wire::{self, Conn};

/// The socket the worker connects back to.
pub const SOCKET_ENV: &str = "FTS_RENDER_WORKER_SOCKET";
/// Where it writes its crash report.
pub const REPORT_ENV: &str = "FTS_RENDER_WORKER_REPORT";
/// `1`: it honours `fault` requests (tests).
pub const FAULTS_ENV: &str = "FTS_RENDER_WORKER_FAULTS";

/// Whether this process is a render worker: its renders never go to
/// another worker.
pub static IS_WORKER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The line, shared between the request loop and the blob fetcher.
type Line = Arc<Mutex<Conn>>;

/// A plugin state blob, from the worker's own memory or the engine.
struct AskedBlobs {
    line: Line,
    held: Mutex<HashMap<String, Vec<u8>>>,
}

impl Blobs for AskedBlobs {
    fn put(&self, bytes: &[u8]) -> String {
        let name = session_format::store::hash_of(bytes);
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(name.clone(), bytes.to_vec());
        name
    }

    fn get(&self, name: &str) -> Option<Vec<u8>> {
        if let Some(b) = self
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(name)
        {
            return Some(b.clone());
        }
        let answer = {
            let mut line = self.line.lock().unwrap_or_else(PoisonError::into_inner);
            line.send(&json!({ "need": "blob", "name": name })).ok()?;
            line.recv().ok()?
        };
        let bytes = wire::unb64(answer.get("blob")?.as_str()?).ok()?;
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(name.to_owned(), bytes.clone());
        Some(bytes)
    }
}

/// The song the worker holds.
struct Held {
    /// The live song's guid, as the engine named it.
    key: String,
    /// The worker's own project guid for it.
    project: String,
    bridge: Bridge,
    /// The facade the bridge's project is served by: kept as long as it.
    _facade: daw::standalone::bootstrap::InProcessDaw,
}

/// The worker's `main`, when [`SOCKET_ENV`] says this process is one.
/// `None` when it is not.
#[must_use]
pub fn main_if_worker() -> Option<Result<(), String>> {
    let socket = std::env::var_os(SOCKET_ENV)?;
    Some(main(PathBuf::from(socket)))
}

fn main(socket: PathBuf) -> Result<(), String> {
    IS_WORKER.store(true, std::sync::atomic::Ordering::SeqCst);
    init_tracing();
    if let Some(report) = std::env::var_os(REPORT_ENV) {
        super::crash::install(std::path::Path::new(&report))?;
    }
    let stream = std::os::unix::net::UnixStream::connect(&socket)
        .map_err(|e| format!("{}: {e}", socket.display()))?;
    let line: Line = Arc::new(Mutex::new(Conn::new(stream).map_err(|e| e.to_string())?));
    let daw = crate::open::start_engine().map_err(|e| e.to_string())?;
    let runtime = crate::open::runtime().ok_or("the worker's engine has no runtime")?;
    lock(&line)
        .send(&json!({ "hello": { "pid": std::process::id() } }))
        .map_err(|e| e.to_string())?;
    let blobs = Arc::new(AskedBlobs {
        line: Arc::clone(&line),
        held: Mutex::default(),
    });
    let faults = std::env::var(FAULTS_ENV).is_ok_and(|v| v == "1");
    let mut held: Option<Held> = None;
    loop {
        let request = match lock(&line).recv() {
            Ok(r) => r,
            // The engine went: so does the worker.
            Err(_) => return Ok(()),
        };
        let answer = match request.get("op").and_then(Value::as_str) {
            Some("ping") => Ok(json!({ "pid": std::process::id() })),
            Some("song") => runtime.block_on(song(&daw, &request, &blobs, &mut held)),
            Some("update") => runtime.block_on(update(&request, &mut held)),
            Some("run") => run(runtime, &request, held.as_ref()),
            Some("fault") if faults => fault(&request),
            other => Err(format!("no such request: {other:?}")),
        };
        if lock(&line).send(&wire::done(answer)).is_err() {
            return Ok(());
        }
    }
}

fn lock(line: &Line) -> std::sync::MutexGuard<'_, Conn> {
    line.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Build the song the request carries, replacing the one held.
async fn song(
    daw: &daw::standalone::Standalone,
    request: &Value,
    blobs: &Arc<AskedBlobs>,
    held: &mut Option<Held>,
) -> Result<Value, String> {
    let began = std::time::Instant::now();
    let text = |k: &str| request.get(k).and_then(Value::as_str).unwrap_or_default();
    let snapshot = wire::unb64(text("snapshot"))?;
    let media_dir = PathBuf::from(text("media_dir"));
    if let Some(old) = held.take() {
        drop(old.bridge);
        let _ = daw::service::Projects::close(daw, &old.project);
    }
    let loro = LoroDoc::new();
    loro.import(&snapshot).map_err(|e| e.to_string())?;
    let doc = SessionDoc::from_loro(loro);
    let info = daw::service::Projects::create(daw).ok_or("the worker's engine made no project")?;
    let project = info.guid.clone();
    let name = text("name").to_owned();
    daw.write_project(&project, |p| p.info.name = name);
    daw.media_bay().set_file_resolver(Box::new(
        daw::standalone::media_bay::ProjectRelativeResolver::new(media_dir.clone()),
    ));
    let facade = daw::standalone::bootstrap::build_in_process_daw(daw.clone())
        .await
        .map_err(|e| e.to_string())?;
    let handle = facade
        .daw
        .project(project.clone())
        .await
        .map_err(|e| e.to_string())?;
    let direct: Arc<dyn session::sync::engine::Direct> = Arc::new(daw.clone());
    let as_blobs: Arc<dyn Blobs> = blobs.clone();
    let bridge = Bridge::join(
        handle,
        MediaRoot(media_dir.clone()),
        doc,
        as_blobs,
        Some(direct),
    )
    .await
    .map_err(|e| e.to_string())?;
    daw::standalone::project_loader::anchor_media(daw, &project, &media_dir);
    let audio = daw::standalone::audio_engine::materialize::materialize_via_bay(daw, &project)
        .map_err(|e| format!("the song's media did not materialize: {e}"))?;
    crate::open::switch_to(daw, &project, false);
    let vv = bridge.doc().loro().oplog_vv().encode();
    *held = Some(Held {
        key: text("key").to_owned(),
        project: project.clone(),
        bridge,
        _facade: facade,
    });
    Ok(json!({
        "vv": wire::b64(&vv),
        "project": project,
        "media_failed": audio.failed.len(),
        "ms": began.elapsed().as_secs_f64() * 1e3,
    }))
}

/// Apply the doc's changes the request carries to the song held.
async fn update(request: &Value, held: &mut Option<Held>) -> Result<Value, String> {
    let began = std::time::Instant::now();
    let held = held.as_mut().ok_or("no song is held")?;
    let bytes = wire::unb64(
        request
            .get("update")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )?;
    held.bridge
        .doc()
        .loro()
        .import(&bytes)
        .map_err(|e| e.to_string())?;
    let changes = held
        .bridge
        .remote_changed()
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "vv": wire::b64(&held.bridge.doc().loro().oplog_vv().encode()),
        "changes": changes.len(),
        "key": held.key,
        "ms": began.elapsed().as_secs_f64() * 1e3,
    }))
}

/// Run an action on the song held, as the live engine's `DAW_RUN` does.
fn run(
    runtime: &'static tokio::runtime::Runtime,
    request: &Value,
    held: Option<&Held>,
) -> Result<Value, String> {
    let held = held.ok_or("no song is held")?;
    let id = request.get("id").and_then(Value::as_str).ok_or("no id")?;
    let args = request.get("args").cloned().unwrap_or(Value::Null);
    let bytes = if args.is_null() {
        Vec::new()
    } else {
        args.to_string().into_bytes()
    };
    let project = held.project.clone();
    let id = id.to_owned();
    let out = runtime.block_on(async move {
        crate::off_loop::blocking(move || {
            crate::open::in_song(&project, || {
                tokio::runtime::Handle::current()
                    .block_on(crate::actions::list().run_json_async(&id, &bytes))
                    .map_err(|e| e.to_string())
            })
        })
        .await
    })??;
    serde_json::from_slice(&out).map_err(|e| e.to_string())
}

/// Crash as a plugin would, naming it (tests).
fn fault(request: &Value) -> Result<Value, String> {
    let plugin = request
        .get("plugin")
        .and_then(Value::as_str)
        .unwrap_or("Test Crash");
    let _processing = super::crash::Processing::enter(plugin);
    match request.get("kind").and_then(Value::as_str) {
        Some("abort") => std::process::abort(),
        _ => {
            // SAFETY: none: this is the crash the request asks for.
            unsafe { std::ptr::null_mut::<u8>().write_volatile(1) };
            Err("the fault did not crash".to_owned())
        }
    }
}

/// The worker's tracing: to its stderr (the engine's log), and exported as
/// service `studio-render-worker` when an OTLP endpoint is named, at the
/// engine's level (`FTS_ENGINE_LOG`). A test's subscriber, set first, wins.
fn init_tracing() {
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::util::SubscriberInitExt as _;
    let level = std::env::var("FTS_ENGINE_LOG").unwrap_or_else(|_| "info,vox_core=warn".to_owned());
    let _ = tracing_subscriber::registry()
        .with(crate::telemetry::with_directives(
            tracing_subscriber::EnvFilter::new(&level),
        ))
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(std::io::stderr),
        )
        .with(crate::telemetry::otel("studio-render-worker"))
        .try_init();
    crate::telemetry::install_panic_hook();
}
