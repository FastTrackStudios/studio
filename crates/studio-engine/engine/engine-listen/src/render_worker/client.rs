//! The engine's side of its render worker: spawning it, keeping its song
//! in step, sending it renders, and noticing when it dies.
//!
//! One worker per engine, one request at a time. It is spawned on the
//! first render routed to it and kept warm. When it dies mid-call, the
//! call answers `plugin_crashed` naming the plugin from the worker's crash
//! report (`super::crash`), and a fresh worker is started behind it. The
//! song is synced again on the next call (a snapshot, then updates).

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::crash::Report;
use super::serve::{FAULTS_ENV, REPORT_ENV, SOCKET_ENV};
use super::wire::{self, Conn};

/// How long a spawned worker has to connect back.
const CONNECT_WAIT: Duration = Duration::from_secs(60);
/// The longest one request may take before the worker is taken for hung
/// (`FTS_RENDER_WORKER_TIMEOUT_S` to change it).
const REQUEST_WAIT: Duration = Duration::from_secs(1800);
/// More crashes than this inside [`CRASH_WINDOW`] and no fresh worker is
/// started behind the last: the next call starts one.
const CRASH_LIMIT: usize = 3;
const CRASH_WINDOW: Duration = Duration::from_secs(60);

/// The song a worker renders: what it is synced from. The live engine's is
/// [`super::LiveSong`]; a test's holds a doc of its own.
pub trait Song: Send + Sync {
    /// What names it (the live song's guid).
    fn key(&self) -> String;
    fn name(&self) -> String;
    /// The folder its media resolves against.
    fn media_dir(&self) -> PathBuf;
    /// Its doc whole: the snapshot and its version.
    ///
    /// # Errors
    /// It could not be read.
    fn snapshot(&self) -> Result<(Vec<u8>, Vec<u8>), String>;
    /// What its doc holds beyond version `vv`, and its version now;
    /// `None` when nothing.
    ///
    /// # Errors
    /// It could not be read, or `vv` is not a version.
    fn updates_since(&self, vv: &[u8]) -> Result<Option<(Vec<u8>, Vec<u8>)>, String>;
    /// A plugin state blob its doc names.
    fn blob(&self, name: &str) -> Option<Vec<u8>>;
}

/// How a worker is started: this program again, in worker mode.
#[derive(Clone, Debug)]
pub struct Spawner {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// More environment for it (a test's).
    pub env: Vec<(String, String)>,
}

impl Spawner {
    /// This executable (or `FTS_RENDER_WORKER_EXE`) with `--render-worker`.
    #[must_use]
    pub fn this_program() -> Self {
        let program = std::env::var_os("FTS_RENDER_WORKER_EXE")
            .map(PathBuf::from)
            .or_else(|| std::env::current_exe().ok())
            .unwrap_or_else(|| PathBuf::from("daw-extension"));
        Self {
            program,
            args: vec!["--render-worker".to_owned()],
            env: Vec::new(),
        }
    }
}

/// Why a call to the worker failed.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    /// The worker died while the call ran: a plugin crashed (named when
    /// known).
    PluginCrashed {
        plugin: Option<super::crash::Plugin>,
        signal: Option<String>,
        op: String,
    },
    /// It took longer than allowed; it was stopped.
    TimedOut { op: String, after_s: f64 },
    /// No worker could be had.
    Unavailable(String),
    /// The worker answered with an error (the action's own).
    Failed(String),
}

impl Failure {
    /// The error's code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::PluginCrashed { .. } => "plugin_crashed",
            Self::TimedOut { .. } => "worker_timeout",
            Self::Unavailable(_) => "worker_unavailable",
            Self::Failed(_) => "failed",
        }
    }

    /// The message an action answers with: its code first, so a caller
    /// can tell it apart.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::PluginCrashed { plugin, signal, op } => {
                let what = match plugin {
                    Some(p) => match &p.file {
                        Some(f) => format!("while processing \"{}\" ({f})", p.name),
                        None => format!("while processing \"{}\"", p.name),
                    },
                    None => "(the plugin it was processing is not known)".to_owned(),
                };
                format!(
                    "plugin_crashed: the render worker died{} {what} during {op}; the live session is unaffected and a fresh worker starts",
                    signal
                        .as_ref()
                        .map(|s| format!(" of {s}"))
                        .unwrap_or_default()
                )
            }
            Self::TimedOut { op, after_s } => format!(
                "worker_timeout: {op} took more than {after_s:.0} s in the render worker; it was stopped and a fresh one starts"
            ),
            Self::Unavailable(why) => format!("worker_unavailable: {why}"),
            Self::Failed(why) => why.clone(),
        }
    }
}

/// A worker that runs.
struct Live {
    child: Child,
    conn: Conn,
    pid: u32,
    report: PathBuf,
    socket: PathBuf,
    /// The song it holds and the version it is at.
    synced: Option<(String, Vec<u8>)>,
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket);
        let _ = std::fs::remove_file(&self.report);
    }
}

/// What the worker is, for `RENDER_WORKER`.
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub pid: Option<u32>,
    pub song: Option<String>,
    pub spawns: u64,
    pub restarts: u64,
    pub renders: u64,
    pub last_crash: Option<String>,
    pub last_spawn_ms: Option<f64>,
    pub last_sync_ms: Option<f64>,
}

/// The engine's worker.
pub struct Worker {
    spawner: Spawner,
    dir: PathBuf,
    live: Option<Live>,
    crashes: Vec<Instant>,
    serial: u64,
    allow_faults: bool,
}

static STATUS: Mutex<Status> = Mutex::new(Status {
    pid: None,
    song: None,
    spawns: 0,
    restarts: 0,
    renders: 0,
    last_crash: None,
    last_spawn_ms: None,
    last_sync_ms: None,
});

/// What the worker is now.
#[must_use]
pub fn status() -> Status {
    STATUS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

fn set_status(change: impl FnOnce(&mut Status)) {
    change(&mut STATUS.lock().unwrap_or_else(PoisonError::into_inner));
}

/// The folder the worker's socket and crash report go in.
fn default_dir() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|d| !d.is_empty())
        .map_or_else(std::env::temp_dir, PathBuf::from);
    base.join("fts")
        .join(format!("render-worker-{}", std::process::id()))
}

fn request_wait() -> Duration {
    std::env::var("FTS_RENDER_WORKER_TIMEOUT_S")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|s| *s > 0.0)
        .map_or(REQUEST_WAIT, Duration::from_secs_f64)
}

impl Worker {
    #[must_use]
    pub fn new(spawner: Spawner) -> Self {
        Self {
            spawner,
            dir: default_dir(),
            live: None,
            crashes: Vec::new(),
            serial: 0,
            allow_faults: false,
        }
    }

    /// The same, honouring `fault` requests (tests).
    #[must_use]
    pub fn allowing_faults(mut self) -> Self {
        self.allow_faults = true;
        self
    }

    /// Start workers with `spawner` from now on, honouring `fault`
    /// requests or not (tests: the test binary as the worker).
    pub fn configure(&mut self, spawner: Spawner, allow_faults: bool) {
        self.stop();
        self.spawner = spawner;
        self.allow_faults = allow_faults;
    }

    /// The running worker's pid.
    #[must_use]
    pub fn pid(&self) -> Option<u32> {
        self.live.as_ref().map(|l| l.pid)
    }

    /// Stop the worker (the next call starts one).
    pub fn stop(&mut self) {
        self.live = None;
        set_status(|s| {
            s.pid = None;
            s.song = None;
        });
    }

    /// A worker, started now if none runs.
    ///
    /// # Errors
    /// It could not be started or did not connect back.
    pub fn ensure(&mut self) -> Result<(), Failure> {
        if let Some(live) = &mut self.live {
            match live.child.try_wait() {
                Ok(None) => return Ok(()),
                _ => self.live = None,
            }
        }
        let span = tracing::info_span!(
            "render_worker.spawn",
            worker.pid = tracing::field::Empty,
            worker.spawn_ms = tracing::field::Empty,
        );
        let _in = span.enter();
        let began = Instant::now();
        let live = self.spawn().map_err(Failure::Unavailable)?;
        let ms = began.elapsed().as_secs_f64() * 1e3;
        span.record("worker.pid", live.pid);
        span.record("worker.spawn_ms", ms);
        set_status(|s| {
            s.pid = Some(live.pid);
            s.song = None;
            s.spawns += 1;
            s.last_spawn_ms = Some(ms);
        });
        self.live = Some(live);
        Ok(())
    }

    fn spawn(&mut self) -> Result<Live, String> {
        use std::os::unix::net::UnixListener;
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        self.serial += 1;
        let socket = self.dir.join(format!("w{}.sock", self.serial));
        let report = self.dir.join(format!("w{}.crash", self.serial));
        let _ = std::fs::remove_file(&socket);
        let _ = std::fs::remove_file(&report);
        let listener =
            UnixListener::bind(&socket).map_err(|e| format!("{}: {e}", socket.display()))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let mut command = Command::new(&self.spawner.program);
        command
            .args(&self.spawner.args)
            .env(SOCKET_ENV, &socket)
            .env(REPORT_ENV, &report)
            // The worker serves nothing and plays nothing.
            .env("FTS_MCP", "0")
            .env("FTS_ENGINE_SILENT", "1")
            .env_remove("ARCHITECT_EXT_SOCKET")
            .stdin(Stdio::null());
        if self.allow_faults {
            command.env(FAULTS_ENV, "1");
        }
        for (k, v) in &self.spawner.env {
            command.env(k, v);
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("{}: {e}", self.spawner.program.display()))?;
        let deadline = Instant::now() + CONNECT_WAIT;
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.to_string()),
            }
            if let Ok(Some(exit)) = child.try_wait() {
                return Err(format!("the worker exited before it connected ({exit})"));
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                return Err(format!(
                    "the worker did not connect within {} s",
                    CONNECT_WAIT.as_secs()
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        stream.set_nonblocking(false).map_err(|e| e.to_string())?;
        let mut conn = Conn::new(stream).map_err(|e| e.to_string())?;
        conn.set_timeout(Some(CONNECT_WAIT))
            .map_err(|e| e.to_string())?;
        let hello = conn
            .recv()
            .map_err(|e| format!("no hello from the worker: {e}"))?;
        let pid = hello["hello"]["pid"]
            .as_u64()
            .and_then(|p| u32::try_from(p).ok())
            .unwrap_or_else(|| child.id());
        Ok(Live {
            child,
            conn,
            pid,
            report,
            socket,
            synced: None,
        })
    }

    /// Run action `id` with `args` (JSON) on `song`, in the worker: synced
    /// first, then run.
    ///
    /// # Errors
    /// The worker crashed or hung (it is replaced), could not be had, or
    /// the action failed there.
    pub fn run(&mut self, song: &dyn Song, id: &str, args: &[u8]) -> Result<Vec<u8>, Failure> {
        self.ensure()?;
        self.sync(song)?;
        let span = tracing::info_span!(
            "render_worker.render",
            action.id = id,
            worker.pid = self.pid(),
            worker.ms = tracing::field::Empty,
        );
        let _in = span.enter();
        let began = Instant::now();
        let args: Value = if args.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(args).map_err(|e| Failure::Failed(e.to_string()))?
        };
        let out = self.ask(song, &json!({ "op": "run", "id": id, "args": args }), id)?;
        span.record("worker.ms", began.elapsed().as_secs_f64() * 1e3);
        set_status(|s| s.renders += 1);
        serde_json::to_vec(&out).map_err(|e| Failure::Failed(e.to_string()))
    }

    /// Bring the worker's song up to `song`: the whole doc when it holds
    /// another (or none), else what changed since its version.
    ///
    /// # Errors
    /// As [`Self::run`].
    pub fn sync(&mut self, song: &dyn Song) -> Result<(), Failure> {
        let key = song.key();
        let held = self.live.as_ref().and_then(|l| l.synced.clone());
        let span = tracing::info_span!(
            "render_worker.sync",
            sync.kind = tracing::field::Empty,
            sync.bytes = tracing::field::Empty,
            sync.changes = tracing::field::Empty,
            sync.ms = tracing::field::Empty,
        );
        let _in = span.enter();
        let began = Instant::now();
        let (request, kind, bytes) = match held {
            Some((k, vv)) if k == key => {
                match song.updates_since(&vv).map_err(Failure::Unavailable)? {
                    None => {
                        span.record("sync.kind", "none");
                        return Ok(());
                    }
                    Some((update, _)) => (
                        json!({ "op": "update", "update": wire::b64(&update) }),
                        "update",
                        update.len(),
                    ),
                }
            }
            _ => {
                let (snapshot, _) = song.snapshot().map_err(Failure::Unavailable)?;
                (
                    json!({
                        "op": "song",
                        "key": key,
                        "name": song.name(),
                        "media_dir": song.media_dir(),
                        "snapshot": wire::b64(&snapshot),
                    }),
                    "snapshot",
                    snapshot.len(),
                )
            }
        };
        span.record("sync.kind", kind);
        span.record("sync.bytes", bytes);
        let answer = self.ask(song, &request, kind)?;
        let vv = answer["vv"]
            .as_str()
            .map(wire::unb64)
            .transpose()
            .map_err(Failure::Failed)?
            .unwrap_or_default();
        if let Some(changes) = answer["changes"].as_u64() {
            span.record("sync.changes", changes);
        }
        if let Some(live) = &mut self.live {
            live.synced = Some((key.clone(), vv));
        }
        let ms = began.elapsed().as_secs_f64() * 1e3;
        span.record("sync.ms", ms);
        set_status(|s| {
            s.song = Some(key);
            s.last_sync_ms = Some(ms);
        });
        Ok(())
    }

    /// Crash the worker as a plugin named `plugin` would (tests).
    ///
    /// # Errors
    /// Always, when it crashes: [`Failure::PluginCrashed`].
    pub fn fault(&mut self, song: &dyn Song, kind: &str, plugin: &str) -> Result<Value, Failure> {
        self.ensure()?;
        self.ask(
            song,
            &json!({ "op": "fault", "kind": kind, "plugin": plugin }),
            "fault",
        )
    }

    /// Send `request` and wait for its end, answering the worker's asks
    /// for blobs on the way.
    fn ask(&mut self, song: &dyn Song, request: &Value, op: &str) -> Result<Value, Failure> {
        let wait = request_wait();
        let result = {
            let live = self
                .live
                .as_mut()
                .ok_or_else(|| Failure::Unavailable("no worker".to_owned()))?;
            exchange(&mut live.conn, song, request, wait)
        };
        match result {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => Err(Failure::Failed(e)),
            Err(e) => Err(self.lost(op, &e, wait)),
        }
    }

    /// The worker went (or hung) during `op`: why, and a fresh one behind.
    fn lost(&mut self, op: &str, error: &std::io::Error, wait: Duration) -> Failure {
        let timed_out = matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        );
        let Some(mut live) = self.live.take() else {
            return Failure::Unavailable(error.to_string());
        };
        if timed_out {
            let _ = live.child.kill();
        }
        let status = wait_exit(&mut live.child, Duration::from_secs(5));
        let report = std::fs::read_to_string(&live.report)
            .map(|t| Report::parse(&t))
            .unwrap_or_default();
        let signal = report
            .signal_name()
            .map(str::to_owned)
            .or_else(|| status.and_then(signal_of));
        let failure = if timed_out {
            Failure::TimedOut {
                op: op.to_owned(),
                after_s: wait.as_secs_f64(),
            }
        } else {
            Failure::PluginCrashed {
                plugin: report.plugin(),
                signal: signal.clone(),
                op: op.to_owned(),
            }
        };
        drop(live);
        let span = tracing::info_span!(
            "render_worker.crash",
            crash.code = failure.code(),
            crash.op = op,
            crash.signal = signal.as_deref().unwrap_or(""),
            crash.plugin = tracing::field::Empty,
            crash.plugin_file = tracing::field::Empty,
        );
        if let Failure::PluginCrashed {
            plugin: Some(p), ..
        } = &failure
        {
            span.record("crash.plugin", p.name.as_str());
            if let Some(f) = &p.file {
                span.record("crash.plugin_file", f.as_str());
            }
        }
        let _in = span.enter();
        tracing::warn!(
            crash.code = failure.code(),
            crash.op = op,
            "render worker: lost; the live session is unaffected"
        );
        restarts_metric(failure.code());
        let now = Instant::now();
        self.crashes
            .retain(|t| now.duration_since(*t) < CRASH_WINDOW);
        self.crashes.push(now);
        set_status(|s| {
            s.pid = None;
            s.song = None;
            s.restarts += 1;
            s.last_crash = Some(failure.message());
        });
        // A fresh worker behind it, unless it keeps dying: then the next
        // call starts one.
        if self.crashes.len() <= CRASH_LIMIT {
            let _ = self.ensure();
        }
        failure
    }
}

/// One request's exchange: `Ok(Ok)` its answer, `Ok(Err)` the worker's
/// error, `Err` the line broke (the worker died or hung).
fn exchange(
    conn: &mut Conn,
    song: &dyn Song,
    request: &Value,
    wait: Duration,
) -> std::io::Result<Result<Value, String>> {
    conn.set_timeout(Some(wait))?;
    conn.send(request)?;
    loop {
        let message = conn.recv()?;
        if let Some(done) = message.get("done") {
            return Ok(match done.get("err") {
                Some(e) => Err(e.as_str().unwrap_or("the worker failed").to_owned()),
                None => Ok(done.get("ok").cloned().unwrap_or(Value::Null)),
            });
        }
        if message.get("need").and_then(Value::as_str) == Some("blob") {
            let name = message["name"].as_str().unwrap_or_default();
            let blob = song.blob(name).map(|b| wire::b64(&b));
            conn.send(&json!({ "blob": blob }))?;
        }
    }
}

fn wait_exit(child: &mut Child, most: Duration) -> Option<std::process::ExitStatus> {
    let until = Instant::now() + most;
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        if Instant::now() > until {
            let _ = child.kill();
            return child.wait().ok();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn signal_of(status: std::process::ExitStatus) -> Option<String> {
    use std::os::unix::process::ExitStatusExt as _;
    let sig = status.signal()?;
    Some(
        Report {
            signal: Some(sig),
            ..Report::default()
        }
        .signal_name()
        .map_or_else(|| format!("signal {sig}"), str::to_owned),
    )
}

/// `studio.render_worker.restarts`, by why (`plugin_crashed`,
/// `worker_timeout`).
fn restarts_metric(why: &'static str) {
    use std::sync::OnceLock;
    static COUNTER: OnceLock<opentelemetry::metrics::Counter<u64>> = OnceLock::new();
    COUNTER
        .get_or_init(|| {
            opentelemetry::global::meter("studio-engine")
                .u64_counter("studio.render_worker.restarts")
                .with_description("Render worker restarts after it died or hung, by `reason`")
                .build()
        })
        .add(1, &[opentelemetry::KeyValue::new("reason", why)]);
}

/// The process's worker, started as this program again.
pub fn global() -> &'static Mutex<Worker> {
    static WORKER: std::sync::LazyLock<Mutex<Worker>> =
        std::sync::LazyLock::new(|| Mutex::new(Worker::new(Spawner::this_program())));
    &WORKER
}
