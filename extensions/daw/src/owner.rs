//! Who owns the engine: the app starts it, nobody else has to.
//!
//! Architect starts a process extension by binding a socket and running
//! its program with that socket in `ARCHITECT_EXT_SOCKET`; the program
//! connects back and serves the host until the host goes. If the engine
//! were that program, closing the window (a dx rebuild, a hot-patch that
//! restarts, a plain reload) would stop the audio and lose the open song.
//! So `daw-extension` is two programs in one binary:
//!
//! - **The launcher** (no argument: what the host runs). It finds the
//!   running engine through its control socket and asks it to serve this
//!   host ([`attach`]); with none running it spawns one first
//!   ([`spawn_engine`]: this binary with `--engine`, as a transient
//!   systemd user service where there is one, else in a session of its
//!   own, so it outlives the window, the launcher and whatever supervises
//!   them). It then waits until
//!   the engine is done with this host, so the host sees one live process
//!   per window, as before.
//! - **The engine** (`--engine`). It takes an exclusive lock on
//!   `engine.lock` for its whole life (a second engine finds the lock taken
//!   and exits: never two), starts the engine, and serves one host at a
//!   time on the control socket, then waits for the next.
//!
//! Where: `FTS_DAW_ENGINE_DIR`, else `$XDG_RUNTIME_DIR/fts/daw`, else a
//! folder per user in the temp dir. There: `engine.lock`, `control.sock`,
//! `engine.json` (pid, build, log) and `engine.log`.
//!
//! Stopping it: `daw-extension --stop` (or `just engine-stop`, or
//! `systemctl --user stop studio-engine` where systemd started it);
//! `--status` says whether one runs. An engine of another build (the extension was
//! rebuilt) is replaced when a window attaches: it stops, and a new one
//! starts, as `studio_engine::link::plan`'s `Replace`.

use architect_extensions_dioxus::guest::GuestExt as _;
use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value as Json, json};

/// The env var Architect names the host's socket in.
const HOST_SOCKET: &str = "ARCHITECT_EXT_SOCKET";
/// How long a launcher waits for a spawned engine to answer.
const SPAWN_WAIT: Duration = Duration::from_secs(120);

/// The folder an engine and its launchers meet in.
#[must_use]
pub fn dir() -> PathBuf {
    if let Some(d) = std::env::var_os("FTS_DAW_ENGINE_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(d);
    }
    if let Some(r) = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(r).join("fts").join("daw");
    }
    // SAFETY: getuid has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    std::env::temp_dir().join(format!("fts-daw-{uid}"))
}

fn control(dir: &Path) -> PathBuf {
    dir.join("control.sock")
}

/// What an engine answers a launcher's request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// It serves the host at the socket asked; the launcher waits.
    Attached,
    /// Another window has it; one host at a time.
    Busy,
    /// It is of another build: it stops, and the launcher starts this one.
    Replace,
    /// Asked to stop: it goes.
    Stopping,
    /// The request made no sense.
    Refused(String),
}

impl Reply {
    fn word(&self) -> String {
        match self {
            Self::Attached => "attached".to_owned(),
            Self::Busy => "busy".to_owned(),
            Self::Replace => "replace".to_owned(),
            Self::Stopping => "stopping".to_owned(),
            Self::Refused(why) => format!("refused {why}"),
        }
    }

    fn parse(line: &str) -> Self {
        match line.trim() {
            "attached" => Self::Attached,
            "busy" => Self::Busy,
            "replace" => Self::Replace,
            "stopping" => Self::Stopping,
            other => Self::Refused(other.strip_prefix("refused ").unwrap_or(other).to_owned()),
        }
    }
}

/// What an engine of build `ours`, serving a host now or not (`busy`),
/// answers `request` (one JSON line: `{"attach": <socket>, "build": …}`
/// or `{"stop": true}`).
#[must_use]
pub fn reply_to(request: &Json, ours: &str, busy: bool) -> Reply {
    if request.get("stop").and_then(Json::as_bool) == Some(true) {
        return Reply::Stopping;
    }
    let Some(socket) = request.get("attach").and_then(Json::as_str) else {
        return Reply::Refused("neither attach nor stop".to_owned());
    };
    if socket.is_empty() {
        return Reply::Refused("an empty socket".to_owned());
    }
    match request.get("build").and_then(Json::as_str) {
        Some(theirs) if !theirs.is_empty() && !ours.is_empty() && theirs != ours => Reply::Replace,
        _ if busy => Reply::Busy,
        _ => Reply::Attached,
    }
}

/// This binary's build (`studio_engine::link::build_id`: its size and
/// modification time).
fn build() -> String {
    session_engine::studio_engine::link::build_id()
}

/// Asks the engine at `dir` one thing, returning its answer and the open
/// stream (the launcher keeps it while attached).
fn ask(dir: &Path, request: &Json) -> std::io::Result<(Reply, BufReader<UnixStream>)> {
    let mut stream = UnixStream::connect(control(dir))?;
    stream.write_all(format!("{request}\n").as_bytes())?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok((Reply::parse(&line), reader))
}

/// The launcher: the host's program. Attaches the running engine to the
/// host, spawning one first if none runs, and returns when the engine is
/// done with this host.
///
/// # Errors
///
/// No host started this process, or no engine could be had.
pub fn launch() -> Result<(), String> {
    let socket = std::env::var(HOST_SOCKET).map_err(|_| {
        format!("{HOST_SOCKET} is not set: a host starts daw-extension (or run it with --engine)")
    })?;
    let dir = dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let request = json!({ "attach": socket, "build": build() });
    let mut spawned = false;
    let deadline = Instant::now() + SPAWN_WAIT;
    loop {
        match ask(&dir, &request) {
            Ok((Reply::Attached, mut reader)) => {
                tracing::info!(engine.dir = %dir.display(), engine.spawned = spawned, "daw: attached to the engine");
                // Until the engine closes this stream: it has finished
                // serving the host (the window went, or the engine did).
                let mut rest = String::new();
                let _ = std::io::Read::read_to_string(&mut reader, &mut rest);
                return Ok(());
            }
            Ok((Reply::Busy, _)) => {
                return Err(
                    "the engine already serves another window (one window at a time)".to_owned(),
                );
            }
            Ok((Reply::Replace, _)) => {
                tracing::info!("daw: the running engine is of another build; replacing it");
                wait_gone(&dir, Duration::from_secs(10));
                spawn_engine(&dir)?;
                spawned = true;
            }
            Ok((Reply::Stopping | Reply::Refused(_), _)) | Err(_) => {
                if !spawned {
                    spawn_engine(&dir)?;
                    spawned = true;
                }
            }
        }
        if Instant::now() > deadline {
            return Err(format!(
                "no engine answered at {} within {} s (its log: {})",
                control(&dir).display(),
                SPAWN_WAIT.as_secs(),
                dir.join("engine.log").display()
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Waits (at most `wait`) until no engine answers at `dir`.
fn wait_gone(dir: &Path, wait: Duration) {
    let until = Instant::now() + wait;
    while Instant::now() < until && UnixStream::connect(control(dir)).is_ok() {
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The systemd unit an engine runs as, when the user's systemd starts it.
const UNIT: &str = "studio-engine";

/// Starts this binary as the engine, outside the window's process tree,
/// its output in `engine.log`: as a transient systemd user service when
/// the user's systemd answers (`systemd-run --user`), else in a session of
/// its own (`setsid`). A session of its own is not enough under a
/// supervisor that stops a process's every descendant (devenv's, a
/// desktop's app manager): the service is no descendant at all.
fn spawn_engine(dir: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let log_path = dir.join("engine.log");
    if let Some(pid) = spawn_as_service(&exe, &log_path) {
        tracing::info!(engine.unit = UNIT, engine.log = %log_path.display(), engine.launcher = pid, "daw: started the engine as a user service");
        return Ok(());
    }
    spawn_in_own_session(&exe, &log_path)
}

/// `systemd-run --user` with this process's environment (the engine needs
/// what the window was given: what to open, its audio, its libraries);
/// `None` when there is no user systemd to ask or it refused.
fn spawn_as_service(exe: &Path, log_path: &Path) -> Option<u32> {
    if std::env::var_os("FTS_DAW_NO_SERVICE").is_some()
        || std::env::var_os("XDG_RUNTIME_DIR").is_none()
    {
        return None;
    }
    let mut command = std::process::Command::new("systemd-run");
    command.args(["--user", "--quiet", "--collect", "--unit", UNIT]);
    command.arg(format!(
        "--property=StandardOutput=append:{}",
        log_path.display()
    ));
    command.arg(format!(
        "--property=StandardError=append:{}",
        log_path.display()
    ));
    for (k, v) in std::env::vars_os() {
        if k == HOST_SOCKET {
            continue;
        }
        let (Some(k), Some(v)) = (k.to_str(), v.to_str()) else {
            continue;
        };
        command.arg(format!("--setenv={k}={v}"));
    }
    command.arg("--").arg(exe).arg("--engine");
    let out = match command.stdin(std::process::Stdio::null()).output() {
        Ok(out) => out,
        Err(e) => {
            tracing::warn!(error = %e, "daw: systemd-run did not start; the engine runs in a session of its own");
            return None;
        }
    };
    if !out.status.success() {
        tracing::warn!(
            engine.systemd_run = %String::from_utf8_lossy(&out.stderr).trim(),
            "daw: systemd-run refused; the engine runs in a session of its own"
        );
        return None;
    }
    Some(std::process::id())
}

/// The engine in a session of its own (no user systemd).
fn spawn_in_own_session(exe: &Path, log_path: &Path) -> Result<(), String> {
    use std::os::unix::process::CommandExt as _;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|e| format!("{}: {e}", log_path.display()))?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let mut command = std::process::Command::new(exe);
    command
        .arg("--engine")
        .env_remove(HOST_SOCKET)
        .stdin(std::process::Stdio::null())
        .stdout(log)
        .stderr(err);
    // SAFETY: setsid is async-signal-safe and touches nothing of the
    // parent's memory; its only failure (already a group leader) cannot
    // happen in a fresh child.
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let child = command
        .spawn()
        .map_err(|e| format!("starting the engine: {e}"))?;
    tracing::info!(engine.pid = child.id(), engine.log = %log_path.display(), "daw: started the engine");
    Ok(())
}

/// The engine's tracing: its log (`engine.log`) and, when an OTLP endpoint
/// is named (`OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`, or
/// `OTEL_EXPORTER_OTLP_ENDPOINT`), every span exported as service
/// `studio-engine`: what a drag, a quantize or an open cost, stage by stage.
/// Its level is the engine's own (`FTS_ENGINE_LOG`, `info`), never the
/// RUST_LOG of the window that started it. The exporter lives for the
/// process.
fn init_tracing() {
    use opentelemetry::trace::TracerProvider as _;
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::util::SubscriberInitExt as _;
    let level = std::env::var("FTS_ENGINE_LOG").unwrap_or_else(|_| "info,vox_core=warn".to_owned());
    let named = |var: &str| std::env::var(var).is_ok_and(|v| !v.trim().is_empty());
    let exporting = named("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT") || named("OTEL_EXPORTER_OTLP_ENDPOINT");
    let provider = exporting
        .then(|| {
            let exporter = opentelemetry_otlp::SpanExporter::builder()
                .with_http()
                .build()
                .ok()?;
            Some(
                opentelemetry_sdk::trace::SdkTracerProvider::builder()
                    .with_batch_exporter(exporter)
                    .with_resource(
                        opentelemetry_sdk::Resource::builder()
                            .with_service_name("studio-engine")
                            .build(),
                    )
                    .build(),
            )
        })
        .flatten();
    let otel = provider
        .as_ref()
        .map(|p| tracing_opentelemetry::layer().with_tracer(p.tracer("studio-engine")));
    // Kept for the process: dropping it would stop the export.
    std::mem::forget(provider);
    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(&level))
        .with(tracing_subscriber::fmt::layer().with_ansi(false))
        .with(otel)
        .try_init();
}

/// The engine: holds the lock, runs the engine, serves one host at a time.
///
/// # Errors
///
/// The folder, the lock or the control socket could not be made.
pub fn engine() -> Result<(), String> {
    let dir = dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // Its output is `engine.log` (the launcher opened it): what it does is
    // read there. A subscriber the engine host installs later wins.
    init_tracing();
    let Some(lock) = take_lock(&dir)? else {
        tracing::info!(engine.dir = %dir.display(), "daw: an engine already runs here; this one exits");
        return Ok(());
    };
    let ours = build();
    let info = json!({
        "pid": std::process::id(),
        "build": ours,
        "log": dir.join("engine.log"),
        "control": control(&dir),
    });
    let _ = std::fs::write(dir.join("engine.json"), format!("{info}\n"));

    crate::start_engine(std::env::var("FTS_DAW_OPEN").ok());
    crate::deck::lead();
    crate::meters::publish();
    // Nothing in a song stays unsaved: every open song is written to its
    // file shortly after it changes.
    session_engine::autosave::start();
    // The engine's algorithms (hit placement, …): the extensions that
    // provide them, and the checkouts run as development copies.
    session_engine::algorithms::start_now();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let result = runtime.block_on(serve_hosts(&dir, &ours));
    let _ = std::fs::remove_file(dir.join("engine.json"));
    let _ = std::fs::remove_file(control(&dir));
    drop(lock);
    result
}

/// The lock that makes this the one engine: `None` when another holds it.
fn take_lock(dir: &Path) -> Result<Option<std::fs::File>, String> {
    use std::os::fd::AsRawFd as _;
    let path = dir.join("engine.lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    // SAFETY: a valid open descriptor; flock only reads it.
    let taken = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    Ok(taken.then_some(file))
}

/// Accepts launchers on the control socket until asked to stop or
/// replaced.
async fn serve_hosts(dir: &Path, ours: &str) -> Result<(), String> {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};

    let path = control(dir);
    // Ours: the lock says no other engine uses it.
    let _ = std::fs::remove_file(&path);
    let listener =
        tokio::net::UnixListener::bind(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    tracing::info!(engine.control = %path.display(), "daw: the engine waits for a window");
    let busy = Arc::new(AtomicBool::new(false));
    loop {
        let (stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let (read, mut write) = stream.into_split();
        let mut line = String::new();
        if tokio::io::BufReader::new(read)
            .read_line(&mut line)
            .await
            .is_err()
        {
            continue;
        }
        let request: Json = serde_json::from_str(&line).unwrap_or(Json::Null);
        let reply = reply_to(&request, ours, busy.load(Ordering::SeqCst));
        let _ = write
            .write_all(format!("{}\n", reply.word()).as_bytes())
            .await;
        match reply {
            Reply::Attached => {
                let Some(socket) = request
                    .get("attach")
                    .and_then(Json::as_str)
                    .map(PathBuf::from)
                else {
                    continue;
                };
                busy.store(true, Ordering::SeqCst);
                let busy = Arc::clone(&busy);
                tokio::spawn(async move {
                    tracing::info!(host.socket = %socket.display(), "daw: serving a window");
                    let served = architect_extensions::guest::Guest::extension(&crate::Daw)
                        .serve_at(&socket)
                        .await;
                    if let Err(e) = served {
                        tracing::warn!(error = %e, "daw: serving the window ended in an error");
                    }
                    tracing::info!("daw: the window went; the engine keeps running");
                    busy.store(false, Ordering::SeqCst);
                    // Dropping `write` ends the launcher.
                    drop(write);
                });
            }
            Reply::Replace | Reply::Stopping => {
                tracing::info!(engine.reply = %reply.word(), "daw: the engine stops");
                return Ok(());
            }
            Reply::Busy | Reply::Refused(_) => {}
        }
    }
}

/// `--stop`: asks the running engine to stop. Whether one was running.
///
/// # Errors
///
/// It answered something other than that it stops.
pub fn stop() -> Result<bool, String> {
    let dir = dir();
    match ask(&dir, &json!({ "stop": true })) {
        Ok((Reply::Stopping, _)) => {
            wait_gone(&dir, Duration::from_secs(10));
            Ok(true)
        }
        Ok((other, _)) => Err(format!("the engine answered {other:?}")),
        Err(_) => Ok(false),
    }
}

/// `--status`: the running engine's `engine.json`, when one answers.
#[must_use]
pub fn status() -> Option<String> {
    let dir = dir();
    UnixStream::connect(control(&dir)).ok()?;
    std::fs::read_to_string(dir.join("engine.json")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_of_the_same_build_attaches_unless_one_is_attached() {
        let ask = json!({"attach": "/tmp/h.sock", "build": "1-2"});
        assert_eq!(reply_to(&ask, "1-2", false), Reply::Attached);
        assert_eq!(reply_to(&ask, "1-2", true), Reply::Busy);
    }

    #[test]
    fn another_build_replaces_the_engine_even_while_busy() {
        let ask = json!({"attach": "/tmp/h.sock", "build": "9-9"});
        assert_eq!(reply_to(&ask, "1-2", true), Reply::Replace);
        assert_eq!(reply_to(&ask, "1-2", false), Reply::Replace);
    }

    #[test]
    fn an_unknown_build_on_either_side_attaches() {
        let ask = json!({"attach": "/tmp/h.sock"});
        assert_eq!(reply_to(&ask, "1-2", false), Reply::Attached);
        let ask = json!({"attach": "/tmp/h.sock", "build": "1-2"});
        assert_eq!(reply_to(&ask, "", false), Reply::Attached);
    }

    #[test]
    fn stop_wins_and_nonsense_is_refused() {
        assert_eq!(reply_to(&json!({"stop": true}), "1", true), Reply::Stopping);
        assert!(matches!(
            reply_to(&json!({}), "1", false),
            Reply::Refused(_)
        ));
        assert!(matches!(
            reply_to(&json!({"attach": ""}), "1", false),
            Reply::Refused(_)
        ));
    }

    #[test]
    fn replies_read_back_as_they_were_written() {
        for r in [
            Reply::Attached,
            Reply::Busy,
            Reply::Replace,
            Reply::Stopping,
            Reply::Refused("x y".to_owned()),
        ] {
            assert_eq!(Reply::parse(&r.word()), r);
        }
    }
}
