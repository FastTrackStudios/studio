//! Native playback through Studio's engine, `studio engine`, a process of
//! its own: found running or spawned, the playing song's one file opened
//! in it, played, paused and moved, and its transport read back to drive
//! Now Playing and the playheads.
//!
//! **Finding it** is `engine_client::studio_engine::link`'s protocol, kept
//! here rather than linked (studio-engine brings the whole DAW, its DSP
//! and its own Blitz with it): an engine says where it is in
//! `<config>/fts/engines/studio-<pid>.json` (its facade's socket, its
//! Action List's MCP address), it is alive when the socket takes a
//! connection, and `FTS_ENGINE=spawn|attach` says whether Studio may start
//! one ([`Mode`], spawn by default). Any running engine is attached to: the
//! app is not the engine's build, so there is no build to match.
//!
//! **Spawning** runs `FTS_STUDIO_BIN engine` when that is set, else the
//! daw extension's engine (`daw-extension --engine`, `FTS_DAW_ENGINE_BIN`
//! or found beside the app or in its build's `target/`), in a process
//! group of its own so the audio outlives the window, with its output in
//! `<cache>/fts/studio-engine.log` and its Action List on a free loopback
//! port (`FTS_MCP_PORT`). The engine picks its audio device itself (its
//! saved choice, else the system's default); Studio never sets one.
//!
//! **Driving it** goes through the engine's Action List over MCP (JSON-RPC
//! on loopback, `tools/call run`): `SESSION_OPEN` / `SESSION_OPENING`,
//! `SESSION_PLAY_FROM`, `SESSION_PLAY_PAUSE`, `SESSION_SEEK` and
//! `SESSION_TRANSPORT`, from a thread of its own ([`Engine`]): the window
//! never waits on the engine. What the window wants ([`Want`]) goes in;
//! what the engine does ([`Snapshot`]) comes out, read by the window on
//! its timer.
//!
//! **On iPhone and iPad** an app runs no second process, so the engine is
//! linked into the app: the daw extension, linked like the views, starts
//! it in this process (`daw_extension::start_engine`) and links the views
//! to it in memory. Studio drives it the same way, its calls answered here
//! ([`Door::Here`]) rather than over loopback; finding and spawning are
//! the desktop's.

// Finding and spawning an engine of its own process: never on iOS.
#![cfg_attr(target_os = "ios", allow(dead_code))]

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::state::EngineStatus;
use crate::trace::Span;

/// How long a spawned engine has to come up.
const SPAWN_WAIT: Duration = Duration::from_secs(90);
/// How long a song has to open.
const OPEN_WAIT: Duration = Duration::from_secs(120);
/// How often the transport is read.
const POLL: Duration = Duration::from_millis(100);

/// Where a running engine is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub pid: u32,
    /// Its facade's socket (none for an engine in this process).
    pub socket: PathBuf,
    pub door: Door,
}

/// Where an engine answers its Action List.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Door {
    /// Its MCP server on loopback: an engine in a process of its own.
    Mcp(String),
    /// This process: the engine linked into the app (iOS), over the
    /// in-memory link it serves the views on.
    Here,
}

/// How Studio has its engine (`FTS_ENGINE`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// A running one, else one spawned.
    Spawn,
    /// A running one only.
    Attach,
}

impl Mode {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "spawn" => Some(Self::Spawn),
            "attach" => Some(Self::Attach),
            _ => None,
        }
    }

    pub fn from_env() -> Self {
        std::env::var("FTS_ENGINE")
            .ok()
            .and_then(|m| Self::parse(&m))
            .unwrap_or(Self::Spawn)
    }

    fn word(self) -> &'static str {
        match self {
            Self::Spawn => "spawn",
            Self::Attach => "attach",
        }
    }
}

// ── finding and spawning ───────────────────────────────────────────────

/// Where engines say where they are: `FTS_ENGINES_DIR` when set (a
/// sandboxed engine's and window's, the engine's `link::DIR_VAR`), else
/// `<config>/fts/engines`.
fn engines_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("FTS_ENGINES_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    Some(crate::storage::config_dir()?.join("fts").join("engines"))
}

/// An engine's file, read.
pub fn parse_found(bytes: &[u8]) -> Option<Found> {
    let v: Value = serde_json::from_slice(bytes).ok()?;
    Some(Found {
        pid: u32::try_from(v.get("pid")?.as_u64()?).ok()?,
        socket: PathBuf::from(v.get("socket")?.as_str()?),
        door: Door::Mcp(v.get("mcp")?.as_str()?.to_string()),
    })
}

/// Whether this instance may attach to an engine: only one whose socket
/// is under its own runtime dir (the engines' files are shared, under the
/// config dir), so a second Studio never takes the person's engine and the
/// person's window never takes a test's or a second instance's.
fn ours(f: &Found) -> bool {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|d| !d.is_empty())
        .is_none_or(|run| f.socket.starts_with(run))
}

/// Every engine that has said where it is (and this instance may use),
/// newest first.
fn found() -> Vec<Found> {
    let Some(entries) = engines_dir().and_then(|d| std::fs::read_dir(d).ok()) else {
        return Vec::new();
    };
    let mut all: Vec<(Found, std::time::SystemTime)> = entries
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_name()?.to_str()?;
            if !(name.starts_with("studio-") && name.ends_with(".json")) {
                return None;
            }
            let when = e.metadata().and_then(|m| m.modified()).ok()?;
            Some((parse_found(&std::fs::read(&p).ok()?)?, when))
        })
        .collect();
    all.sort_by(|a, b| b.1.cmp(&a.1));
    all.into_iter().map(|(f, _)| f).filter(ours).collect()
}

fn alive(f: &Found) -> bool {
    match f.door {
        // It lives as long as the app does.
        Door::Here => true,
        Door::Mcp(_) => std::os::unix::net::UnixStream::connect(&f.socket).is_ok(),
    }
}

/// Whether the engine's Action List is its own: an engine whose port was
/// taken still says the port in its file, and another process answers
/// there. On Linux the listening socket's owner is checked
/// (`/proc/net/tcp`, then the engine's descriptors); elsewhere it is
/// taken on trust.
fn serves_its_own(f: &Found) -> bool {
    let Door::Mcp(mcp) = &f.door else {
        return true;
    };
    if !cfg!(target_os = "linux") {
        return true;
    }
    let Some(port) = split_url(mcp).and_then(|(host, _)| {
        host.rsplit_once(':')
            .and_then(|(_, p)| p.parse::<u16>().ok())
    }) else {
        return false;
    };
    let Ok(table) = std::fs::read_to_string("/proc/net/tcp") else {
        return true;
    };
    let Some(inode) = listening_inode(&table, port) else {
        return false;
    };
    let want = format!("socket:[{inode}]");
    std::fs::read_dir(format!("/proc/{}/fd", f.pid))
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| std::fs::read_link(e.path()).is_ok_and(|l| l.to_string_lossy() == want))
}

/// The inode of the socket listening on `port` in a `/proc/net/tcp` table.
pub fn listening_inode(table: &str, port: u16) -> Option<String> {
    let port = format!("{port:04X}");
    table.lines().skip(1).find_map(|l| {
        let cols: Vec<&str> = l.split_whitespace().collect();
        let local = cols.get(1)?;
        let (_, p) = local.split_once(':')?;
        (p == port && cols.get(3) == Some(&"0A")).then(|| cols.get(9).map(|s| s.to_string()))?
    })
}

/// A running engine Studio can drive.
fn drivable(f: &Found) -> bool {
    alive(f) && serves_its_own(f)
}

/// The daw extension's program (`daw-extension`), as the engine is found
/// (`FTS_DAW_ENGINE_BIN`, beside the app, its build's `target/`).
pub fn daw_extension_bin() -> Option<PathBuf> {
    std::env::var_os("FTS_DAW_ENGINE_BIN")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            let saved = std::env::var_os("FTS_STUDIO_BIN");
            // The finder below, without `FTS_STUDIO_BIN` (another program).
            if saved.is_some() {
                return None;
            }
            engine_program().map(|(bin, _)| bin)
        })
}

/// The program that is the engine, and its arguments.
fn engine_program() -> Option<(PathBuf, Vec<String>)> {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if let Some(bin) = env("FTS_STUDIO_BIN") {
        return Some((bin, vec!["engine".into()]));
    }
    if let Some(bin) = env("FTS_DAW_ENGINE_BIN") {
        return Some((bin, vec!["--engine".into()]));
    }
    // Beside the app, or (a development build) in its build's target/:
    // `target/dx/studio-app/…/studio-app` beside `target/<triple>/<profile>/`.
    let exe = std::env::current_exe().ok()?;
    let beside = exe.with_file_name("daw-extension");
    if beside.is_file() {
        return Some((beside, vec!["--engine".into()]));
    }
    let target = exe
        .ancestors()
        .find(|a| a.file_name().is_some_and(|n| n == "target"))?;
    // The host triple the Justfile builds for (`dev_build`).
    let os = if cfg!(target_os = "macos") {
        "apple-darwin"
    } else {
        "unknown-linux-gnu"
    };
    let host = format!("{}-{os}", std::env::consts::ARCH);
    [
        target.join(&host).join("desktop-dev"),
        target.join("desktop-dev"),
        target.join(&host).join("debug"),
        target.join("debug"),
    ]
    .into_iter()
    .map(|d| d.join("daw-extension"))
    .find(|p| p.is_file())
    .map(|p| (p, vec!["--engine".into()]))
}

fn log_file() -> Option<PathBuf> {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let cache = env("XDG_CACHE_HOME").or_else(|| env("HOME").map(|h| h.join(".cache")))?;
    Some(cache.join("fts").join("studio-engine.log"))
}

/// The engine this Studio started (its program and pid): the one it stops
/// when it closes. An engine it found running is not in here, and is never
/// stopped by it.
static STARTED: std::sync::Mutex<Option<(PathBuf, u32)>> = std::sync::Mutex::new(None);

/// Studio's engine folder (`FTS_DAW_ENGINE_DIR`): the daw extension's
/// engine keeps one engine per folder (a lock), so Studio's starts beside
/// one another app runs. Studio's process carries it too (`main`), so the
/// daw extension's launcher finds this same engine and never starts a
/// second one.
#[must_use]
pub fn engine_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|d| !d.is_empty())
        .map(|run| {
            PathBuf::from(run)
                .join("fts")
                .join(crate::instance::engine_folder())
        })
}

/// Keep the engine running when Studio closes (`1`), or stop it (`0`).
/// Unset: kept by a development build, so a UI rebuild finds the song
/// still loaded and playing; stopped by a release build, so closing Studio
/// closes everything it started.
pub const KEEP_ENGINE: &str = "STUDIO_KEEP_ENGINE";

/// Whether Studio leaves its engine running when it closes.
#[must_use]
pub fn keeps_engine() -> bool {
    match std::env::var(KEEP_ENGINE).as_deref() {
        Ok("1" | "true" | "yes") => true,
        Ok("0" | "false" | "no") => false,
        _ => cfg!(debug_assertions),
    }
}

/// Studio is closing: stops the engine it started (`daw-extension
/// --stop`, its own orderly stop), unless it keeps it ([`keeps_engine`]).
/// An engine it attached to and did not start is left alone.
pub fn stop_started_engine() {
    let started = STARTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    let Some((bin, pid)) = started else { return };
    if keeps_engine() {
        tracing::info!(
            engine.pid = pid,
            "studio: closing, the engine keeps running ({KEEP_ENGINE})"
        );
        return;
    }
    let stopped = std::process::Command::new(&bin)
        .arg("--stop")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match stopped {
        Ok(status) if status.success() => {
            tracing::info!(
                engine.pid = pid,
                "studio: closing, the engine it started is stopped"
            );
        }
        other => {
            tracing::warn!(engine.pid = pid, outcome = ?other, "studio: closing, the engine did not stop")
        }
    }
}

/// A loopback port nobody holds now.
pub fn free_port() -> Option<u16> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .ok()?
        .local_addr()
        .ok()
        .map(|a| a.port())
}

fn spawn(bin: &Path, args: &[String], wait: Duration) -> Result<Found, String> {
    use std::os::unix::process::CommandExt as _;
    let log = log_file().ok_or("no cache folder for the engine's log")?;
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .map_err(|e| e.to_string())?;
    let err = out.try_clone().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(bin);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(out)
        .stderr(err)
        .process_group(0);
    // The daw extension's engine keeps one engine per folder (a lock):
    // Studio's own folder, so it starts beside one another app runs.
    if args.iter().any(|a| a == "--engine")
        && std::env::var_os("FTS_DAW_ENGINE_DIR").is_none()
        && let Some(dir) = engine_dir()
    {
        cmd.env("FTS_DAW_ENGINE_DIR", dir);
    }
    if std::env::var_os("FTS_MCP_PORT").is_none()
        && let Some(port) = free_port()
    {
        cmd.env("FTS_MCP_PORT", port.to_string());
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("{} could not start: {e}", bin.display()))?;
    let pid = child.id();
    // Ours: stopped when Studio closes, unless it keeps its engine.
    *STARTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((bin.to_path_buf(), pid));
    // Reaped when it goes, so it never lingers as a zombie.
    let _ = std::thread::Builder::new()
        .name("engine-child".into())
        .spawn(move || {
            let _ = child.wait();
        });
    let began = Instant::now();
    while began.elapsed() < wait {
        if let Some(up) = found().into_iter().find(|f| f.pid == pid && alive(f)) {
            if !serves_its_own(&up) {
                return Err(format!(
                    "the engine (pid {pid}) came up without its Action List (its port is taken); see {}",
                    log.display()
                ));
            }
            return Ok(up);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(format!(
        "the engine (pid {pid}) did not come up within {} s; see {}",
        wait.as_secs(),
        log.display()
    ))
}

/// On iPhone and iPad: the engine in this process, started by the linked
/// daw extension (once; it starts with the extensions too), up when its
/// Action List answers.
#[cfg(target_os = "ios")]
fn choose(_mode: Mode, span: &mut Span) -> Result<Found, String> {
    span.set("engine.plan", "here");
    daw_extension::start_engine(None);
    let here = Found {
        pid: std::process::id(),
        socket: PathBuf::new(),
        door: Door::Here,
    };
    let began = Instant::now();
    loop {
        match run(&here, "SESSION_TRANSPORT", Value::Null) {
            Ok(_) => return Ok(here),
            Err(e) if began.elapsed() > SPAWN_WAIT => {
                return Err(format!(
                    "the engine in the app did not come up within {} s: {e}",
                    SPAWN_WAIT.as_secs()
                ));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(200)),
        }
    }
}

/// A running engine, or one started: what a playing set needs.
#[cfg(not(target_os = "ios"))]
fn choose(mode: Mode, span: &mut Span) -> Result<Found, String> {
    span.set("engine.mode", mode.word());
    let running = found();
    let undrivable = running
        .iter()
        .filter(|f| alive(f) && !serves_its_own(f))
        .count();
    if undrivable > 0 {
        span.set("engine.undrivable", undrivable as u64);
        tracing::warn!(
            engine.undrivable = undrivable as u64,
            "engine: a running engine serves no Action List of its own (its port is another process's); not attached to"
        );
    }
    if let Some(up) = running.into_iter().find(drivable) {
        span.set("engine.plan", "attach");
        return Ok(up);
    }
    match mode {
        Mode::Attach => {
            span.set("engine.plan", "wait");
            let began = Instant::now();
            while began.elapsed() < Duration::from_secs(10) {
                if let Some(up) = found().into_iter().find(drivable) {
                    return Ok(up);
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            Err("no engine runs to attach to (FTS_ENGINE=attach)".into())
        }
        Mode::Spawn => {
            span.set("engine.plan", "spawn");
            let (bin, args) = engine_program().ok_or(
                "no engine program: build daw-extension, or set FTS_STUDIO_BIN or FTS_DAW_ENGINE_BIN",
            )?;
            spawn(&bin, &args, SPAWN_WAIT)
        }
    }
}

// ── the engine's Action List, over MCP ─────────────────────────────────

/// `http://127.0.0.1:<port>/<path>` as (host:port, path).
fn split_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("http://")?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    Some((host.to_string(), format!("/{path}")))
}

/// An HTTP response's body: chunks joined, an event stream's last `data:`.
pub fn response_body(raw: &[u8]) -> Result<String, String> {
    let text = String::from_utf8_lossy(raw);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or("the engine answered no HTTP")?;
    let status = head.lines().next().unwrap_or_default();
    if !status.contains(" 200") && !status.contains(" 202") {
        return Err(format!("the engine answered {status}"));
    }
    let chunked = head
        .lines()
        .any(|l| l.to_ascii_lowercase().starts_with("transfer-encoding:") && l.contains("chunked"));
    let mut body = if chunked {
        let mut out = String::new();
        let mut rest = body;
        while let Some((size, after)) = rest.split_once("\r\n") {
            let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
            if n == 0 || after.len() < n {
                break;
            }
            out.push_str(&after[..n]);
            rest = after[n..].trim_start_matches("\r\n");
        }
        out
    } else {
        body.to_string()
    };
    if let Some(data) = body
        .lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .last()
        .map(|d| d.trim().to_string())
    {
        body = data;
    }
    Ok(body)
}

/// What an action answered, from its `tools/call` result.
pub fn action_answer(rpc: &Value) -> Result<Value, String> {
    if let Some(e) = rpc.get("error") {
        return Err(e
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("the engine refused the call")
            .to_string());
    }
    let result = rpc.get("result").ok_or("the engine answered no result")?;
    let text = result
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(if text.is_empty() {
            "the action failed".into()
        } else {
            text.to_string()
        });
    }
    if let Some(v) = result.get("structuredContent") {
        return Ok(v.clone());
    }
    Ok(serde_json::from_str(text).unwrap_or(Value::String(text.to_string())))
}

/// Runs action `id` of the engine `found` with `args`, at its door.
fn run(found: &Found, id: &str, args: Value) -> Result<Value, String> {
    match &found.door {
        Door::Mcp(mcp) => run_over_mcp(mcp, id, args),
        Door::Here => run_here(id, &args),
    }
}

/// Runs action `id` of the engine in this process, over the in-memory
/// link the views use.
fn run_here(id: &str, args: &Value) -> Result<Value, String> {
    let out = engine_client::view_client::run(id, args.to_string().as_bytes())?;
    Ok(serde_json::from_str(&out).unwrap_or(Value::String(out)))
}

/// Runs action `id` of the engine at `mcp` with `args`.
fn run_over_mcp(mcp: &str, id: &str, args: Value) -> Result<Value, String> {
    let (host, path) = split_url(mcp).ok_or("the engine's MCP address is not http")?;
    let addr = std::net::ToSocketAddrs::to_socket_addrs(&host)
        .map_err(|e| e.to_string())?
        .next()
        .ok_or("the engine's MCP address does not resolve")?;
    let mut stream = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(2))
        .map_err(|e| format!("the engine's Action List did not answer: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "run", "arguments": { "id": id, "args": args } },
    })
    .to_string();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(|e| e.to_string())?;
    let body = response_body(&raw)?;
    let rpc: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    action_answer(&rpc)
}

// ── the engine, from the window ────────────────────────────────────────

/// The engine the app is linked to.
static LINKED: Mutex<Option<Found>> = Mutex::new(None);

/// The engine the app is linked to now, when it is: the one the session's
/// views attach to (`crate::hosted::use_engine_link`).
pub fn linked() -> Option<Found> {
    LINKED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// What the window wants of the engine.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Want {
    /// The set: each song's one file, in order (none for a song with no
    /// file on this computer), and its name. Empty: the song alone.
    pub set: Vec<Option<PathBuf>>,
    pub set_name: String,
    /// The song's place in the set.
    pub current: usize,
    /// The song's one file, and its title (for the spans).
    pub song: Option<(PathBuf, String)>,
    pub playing: bool,
    /// Where to start when it plays from a song just opened.
    pub from: f64,
    /// Counts the window's wants: a snapshot says which it has acted on.
    pub round: u64,
}

/// What the engine does, as last read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub status: EngineStatus,
    /// The song open in the engine (as the window named it).
    pub song: Option<PathBuf>,
    pub playing: bool,
    pub position: f64,
    /// The last [`Want::round`] acted on.
    pub round: u64,
}

enum Cmd {
    Want(Want),
    Seek(f64),
}

/// The engine's thread, started with the first want that plays.
pub struct Engine {
    send: Mutex<Option<Sender<Cmd>>>,
    snapshot: Arc<Mutex<Snapshot>>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Self {
            send: Mutex::new(None),
            snapshot: Arc::new(Mutex::new(Snapshot::default())),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// What the window wants now. The thread starts the first time the
    /// window wants to play.
    pub fn want(&self, want: Want) {
        let mut send = self.send.lock().unwrap_or_else(PoisonError::into_inner);
        if send.is_none() {
            // A set loads its songs as soon as it is open; a lone song
            // starts the engine when it plays.
            let wanted =
                want.set.iter().any(Option::is_some) || (want.playing && want.song.is_some());
            if !wanted {
                return;
            }
            let (tx, rx) = channel();
            let snapshot = Arc::clone(&self.snapshot);
            let started = std::thread::Builder::new()
                .name("studio-engine-link".into())
                .spawn(move || worker(&rx, &snapshot));
            match started {
                Ok(_) => *send = Some(tx),
                Err(e) => {
                    let mut s = self.snapshot.lock().unwrap_or_else(PoisonError::into_inner);
                    s.status = EngineStatus::Failed(format!("no thread for the engine: {e}"));
                    return;
                }
            }
        }
        if let Some(tx) = send.as_ref() {
            let _ = tx.send(Cmd::Want(want));
        }
    }

    pub fn seek(&self, at: f64) {
        if let Some(tx) = self
            .send
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            let _ = tx.send(Cmd::Seek(at));
        }
    }
}

fn put(snapshot: &Mutex<Snapshot>, f: impl FnOnce(&mut Snapshot)) {
    f(&mut snapshot.lock().unwrap_or_else(PoisonError::into_inner));
}

/// The thread: link, then keep the engine where the window wants it.
fn worker(rx: &Receiver<Cmd>, snapshot: &Mutex<Snapshot>) {
    put(snapshot, |s| s.status = EngineStatus::Linking);
    let mode = Mode::from_env();
    let mut span = Span::root("studio.engine.link");
    let linked = choose(mode, &mut span).and_then(|f| {
        // An engine that has said where it is but whose Action List is not
        // served (its port was taken) cannot be driven.
        run(&f, "SESSION_TRANSPORT", Value::Null).map(|_| f)
    });
    let found = match linked {
        Ok(f) => {
            span.set("engine.pid", f.pid);
            span.set("engine.outcome", "linked");
            f
        }
        Err(why) => {
            span.set("engine.outcome", "failed");
            span.fail(why.clone());
            tracing::warn!(engine.mode = mode.word(), error = %why, "engine: not linked; the playhead follows a timer");
            put(snapshot, |s| s.status = EngineStatus::Failed(why));
            return;
        }
    };
    drop(span);
    *LINKED.lock().unwrap_or_else(PoisonError::into_inner) = Some(found.clone());
    put(snapshot, |s| s.status = EngineStatus::Ready(found.pid));

    let mut want = Want::default();
    let mut opened: Option<PathBuf> = None;
    let mut opened_set: Vec<Option<PathBuf>> = Vec::new();
    // The engine's current song, by its number in the set (1 the first).
    let mut on_number: Option<u32> = None;
    let mut applied = 0u64;
    // A play asked for and not yet heard: (from where, asked again after).
    let mut pending: Option<(f64, Instant, Instant)> = None;
    loop {
        let mut seek = None;
        match rx.recv_timeout(POLL) {
            Ok(cmd) => take(cmd, &mut want, &mut seek),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
        }
        while let Ok(cmd) = rx.try_recv() {
            take(cmd, &mut want, &mut seek);
        }
        if !alive(&found) {
            tracing::warn!(engine.pid = found.pid, "engine: it stopped");
            put(snapshot, |s| {
                s.status = EngineStatus::Failed(format!("the engine (pid {}) stopped", found.pid));
            });
            return;
        }
        // The set first, whole: every song opened in the engine, as the
        // launcher loaded a setlist; then the song, gone to.
        if want.set.iter().any(Option::is_some) && opened_set != want.set {
            let name = want.set_name.clone();
            put(snapshot, |s| s.status = EngineStatus::Opening(name.clone()));
            match open_set(&found, &want.set, &name, snapshot) {
                Ok(()) => {
                    opened_set = want.set.clone();
                    // The engine's set starts at its first song.
                    on_number = Some(1);
                    opened = None;
                    put(snapshot, |s| s.status = EngineStatus::Ready(found.pid));
                }
                Err(why) => {
                    tracing::warn!(studio.set = %name, error = %why, "engine: the set did not open");
                    opened_set = want.set.clone();
                    put(snapshot, |s| {
                        s.status = EngineStatus::Failed(format!("{name} did not open: {why}"));
                        s.song = None;
                    });
                    continue;
                }
            }
        }
        if !opened_set.is_empty() && opened_set == want.set {
            let number = number_in(&want.set, want.current);
            if let (Some(n), Some((path, title))) = (number, want.song.clone())
                && on_number != Some(n)
            {
                let mut span = Span::root("studio.engine.song");
                span.set("studio.song", title.clone());
                span.set("engine.number", u64::from(n));
                match run(&found, "SESSION_SONG", json!({ "to": n.to_string() })) {
                    Ok(_) => {
                        span.set("engine.outcome", "current");
                        on_number = Some(n);
                        opened = Some(path.clone());
                        put(snapshot, |s| {
                            s.song = Some(path);
                            s.position = want.from;
                        });
                        if want.playing {
                            play(&found, &title, want.from);
                            let now = Instant::now();
                            pending = Some((
                                want.from,
                                now + Duration::from_secs(1),
                                now + Duration::from_secs(5),
                            ));
                        }
                        applied = want.round;
                    }
                    Err(e) => {
                        span.set("engine.outcome", "refused");
                        span.fail(e.clone());
                        tracing::warn!(studio.song = %title, error = %e, "engine: the set's song could not be gone to");
                    }
                }
            } else if number.is_none() {
                // A song with no file here: the timer plays it.
                put(snapshot, |s| s.song = None);
            } else if let Some((path, _)) = want.song.clone() {
                put(snapshot, |s| s.song = Some(path));
            }
        } else if let Some((path, title)) = want.song.clone()
            && opened.as_ref() != Some(&path)
        {
            put(snapshot, |s| {
                s.status = EngineStatus::Opening(title.clone())
            });
            match open(&found, &path, &title) {
                Ok(()) => {
                    opened = Some(path.clone());
                    put(snapshot, |s| {
                        s.status = EngineStatus::Ready(found.pid);
                        s.song = Some(path);
                    });
                    if want.playing {
                        play(&found, &title, want.from);
                        let now = Instant::now();
                        pending = Some((
                            want.from,
                            now + Duration::from_secs(1),
                            now + Duration::from_secs(5),
                        ));
                    }
                    applied = want.round;
                }
                Err(why) => {
                    tracing::warn!(studio.song = %title, error = %why, "engine: the song did not open");
                    opened = Some(path);
                    put(snapshot, |s| {
                        s.status = EngineStatus::Failed(format!("{title} did not open: {why}"));
                        s.song = None;
                    });
                }
            }
        }
        let song_open = matches!(
            snapshot
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .status,
            EngineStatus::Ready(_)
        );
        if !song_open {
            continue;
        }
        if let Some(at) = seek
            && let Err(e) = run(&found, "SESSION_SEEK", json!({ "at": at }))
        {
            tracing::warn!(error = %e, "engine: the seek was refused");
        }
        let Ok(t) = run(&found, "SESSION_TRANSPORT", Value::Null) else {
            continue;
        };
        let mut playing = t.get("playing").and_then(Value::as_bool).unwrap_or(false);
        // Stopped, the engine says no position (or its start): where it
        // stopped stands, and playing on starts from there.
        let was = snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .position;
        let mut position = if playing {
            t.get("position").and_then(Value::as_f64).unwrap_or(was)
        } else {
            was
        };
        if let Some(at) = seek {
            position = at;
        }
        // A play not heard yet: asked again each second, the intent shown
        // meanwhile; after five seconds the engine's word stands.
        if let Some((from, again, until)) = pending {
            let now = Instant::now();
            if playing || !want.playing || now > until {
                pending = None;
            } else {
                if now > again {
                    let title = want.song.as_ref().map(|s| s.1.clone()).unwrap_or_default();
                    play(&found, &title, from);
                    pending = Some((from, now + Duration::from_secs(1), until));
                }
                playing = true;
                position = from;
            }
        }
        if want.round != applied {
            let title = want.song.as_ref().map(|s| s.1.clone()).unwrap_or_default();
            if want.playing && !playing {
                play(&found, &title, position);
                let now = Instant::now();
                pending = Some((
                    position,
                    now + Duration::from_secs(1),
                    now + Duration::from_secs(5),
                ));
                playing = true;
            } else if !want.playing && playing {
                let mut span = Span::root("studio.engine.pause");
                span.set("studio.song", title);
                if let Err(e) = run(&found, "SESSION_PLAY_PAUSE", Value::Null) {
                    span.fail(e);
                }
                playing = false;
            }
            // The engine's transport lags a call by a moment: what was
            // asked stands for this round, read again on the next.
            applied = want.round;
        }
        put(snapshot, |s| {
            s.playing = playing;
            s.position = position;
            s.round = applied;
        });
    }
}

fn take(cmd: Cmd, want: &mut Want, seek: &mut Option<f64>) {
    match cmd {
        Cmd::Want(w) => *want = w,
        Cmd::Seek(at) => *seek = Some(at),
    }
}

/// Song `i`'s number in the engine's set (1 the first): songs with no
/// file are not in it.
pub fn number_in(set: &[Option<PathBuf>], i: usize) -> Option<u32> {
    set.get(i)?.as_ref()?;
    u32::try_from(set[..=i].iter().filter(|s| s.is_some()).count()).ok()
}

/// The engine's own setlist of `set` (one song's path a line), where it
/// can be opened from.
fn write_engine_setlist(set: &[Option<PathBuf>], name: &str) -> Result<PathBuf, String> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|d| !d.is_empty())
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("fts")
        .join("studio-app-sets");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let file = dir.join(format!("{}.setlist", safe.trim()));
    let mut text = format!("# {name}, as Studio plays it\n");
    for p in set.iter().flatten() {
        text.push_str(&p.to_string_lossy());
        text.push('\n');
    }
    std::fs::write(&file, text).map_err(|e| e.to_string())?;
    Ok(file)
}

/// Opens the whole set in the engine and waits until every song is open.
fn open_set(
    found: &Found,
    set: &[Option<PathBuf>],
    name: &str,
    snapshot: &Mutex<Snapshot>,
) -> Result<(), String> {
    let mut span = Span::root("studio.engine.set_open");
    span.set("studio.set", name);
    span.set("engine.pid", found.pid);
    let want = set.iter().flatten().count();
    span.set("studio.set.songs", want as u64);
    span.set("studio.set.without_file", (set.len() - want) as u64);
    let began = Instant::now();
    let result = write_engine_setlist(set, name)
        .and_then(|file| open(found, &file, name))
        .and_then(|()| {
            loop {
                let songs = run(&found, "SESSION_SONGS", Value::Null)?;
                let all = songs.as_array().cloned().unwrap_or_default();
                let open = all
                    .iter()
                    .filter(|s| s.get("state").and_then(Value::as_str) == Some("open"))
                    .count();
                put(snapshot, |s| {
                    s.status = EngineStatus::Opening(format!("{name}: {open} of {want} songs"));
                });
                if open >= want {
                    break Ok(());
                }
                if began.elapsed() > OPEN_WAIT * 3 {
                    break Err(format!("{open} of {want} songs opened"));
                }
                std::thread::sleep(Duration::from_millis(300));
            }
        });
    span.set(
        "engine.open_ms",
        u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX),
    );
    match &result {
        Ok(()) => span.set("engine.outcome", "opened"),
        Err(e) => {
            span.set("engine.outcome", "failed");
            span.fail(e.clone());
        }
    }
    result
}

/// Opens `path` in the engine and waits until it is open.
fn open(found: &Found, path: &Path, title: &str) -> Result<(), String> {
    let mut span = Span::root("studio.engine.open");
    span.set("studio.song", title);
    span.set("engine.pid", found.pid);
    let began = Instant::now();
    let path_arg = path.to_string_lossy().to_string();
    if let Err(e) = run(&found, "SESSION_OPEN", json!({ "path": path_arg })) {
        span.set("engine.outcome", "refused");
        span.fail(e.clone());
        return Err(e);
    }
    let result = settle(found, began);
    span.set(
        "engine.open_ms",
        u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX),
    );
    match &result {
        Ok(()) => span.set("engine.outcome", "opened"),
        Err(e) => {
            span.set("engine.outcome", "failed");
            span.fail(e.clone());
        }
    }
    result
}

/// Makes a new session in the engine (`SESSION_NEW` with `args`: from
/// scratch, from a folder of stems or from a `.RPP`) and waits until it is
/// open. Links to the engine first (or starts it), as playing does. Its
/// `SONG_INFO`. Blocks: call it off the UI thread.
pub fn new_session(args: Value) -> Result<Value, String> {
    let mut span = Span::root("studio.engine.new_session");
    let found = match linked() {
        Some(f) => f,
        None => choose(Mode::from_env(), &mut span)?,
    };
    span.set("engine.pid", found.pid);
    if let Some(from) = args.get("from").and_then(Value::as_str) {
        span.set("session.from", from);
    }
    let began = Instant::now();
    let made = run(&found, "SESSION_NEW", args)
        .and_then(|_| settle(&found, began))
        .and_then(|()| run(&found, "SONG_INFO", Value::Null));
    match &made {
        Ok(_) => span.set("engine.outcome", "made"),
        Err(e) => {
            span.set("engine.outcome", "failed");
            span.fail(e.clone());
        }
    }
    made
}

/// Waits until the open the engine began at `began` is done.
fn settle(found: &Found, began: Instant) -> Result<(), String> {
    let mut seen_opening = false;
    loop {
        std::thread::sleep(Duration::from_millis(150));
        let state = run(&found, "SESSION_OPENING", Value::Null)?;
        match state.get("state").and_then(Value::as_str) {
            Some("failed") => {
                break Err(state
                    .get("why")
                    .and_then(Value::as_str)
                    .unwrap_or("it failed")
                    .to_string());
            }
            Some("opening") => seen_opening = true,
            _ if seen_opening || began.elapsed() > Duration::from_millis(1500) => break Ok(()),
            _ => {}
        }
        if began.elapsed() > OPEN_WAIT {
            break Err(format!("still opening after {} s", OPEN_WAIT.as_secs()));
        }
    }
}

fn play(found: &Found, title: &str, from: f64) {
    let mut span = Span::root("studio.engine.play");
    span.set("studio.song", title);
    span.set("engine.pid", found.pid);
    span.set("engine.from_s", from);
    match run(found, "SESSION_PLAY_FROM", json!({ "at": from.max(0.0) })) {
        Ok(_) => span.set("engine.outcome", "playing"),
        Err(e) => {
            span.set("engine.outcome", "refused");
            tracing::warn!(studio.song = %title, error = %e, "engine: play was refused");
            span.fail(e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_engines_file_reads() {
        let f = parse_found(
            br#"{"pid": 12, "socket": "/run/x.sock", "mcp": "http://127.0.0.1:47811/mcp", "ws": null, "build": "b"}"#,
        )
        .unwrap();
        assert_eq!(f.pid, 12);
        assert_eq!(f.door, Door::Mcp("http://127.0.0.1:47811/mcp".into()));
        assert!(parse_found(b"{}").is_none());
    }

    #[test]
    fn an_engine_in_this_process_needs_no_socket_or_port() {
        let here = Found {
            pid: std::process::id(),
            socket: PathBuf::new(),
            door: Door::Here,
        };
        assert!(alive(&here));
        assert!(serves_its_own(&here));
        assert!(drivable(&here));
        let gone = Found {
            pid: 1,
            socket: PathBuf::from("/nonexistent/studio-703.sock"),
            door: Door::Mcp("http://127.0.0.1:1/mcp".into()),
        };
        assert!(!alive(&gone));
    }

    #[test]
    fn the_listening_socket_is_found_by_port() {
        let table = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:BAAB 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 4242 1\n   1: 0100007F:BAAB 0100007F:9C40 01 00000000:00000000 00:00000000 00000000  1000        0 99 1\n";
        assert_eq!(listening_inode(table, 47787).as_deref(), Some("4242"));
        assert_eq!(listening_inode(table, 47811), None);
    }

    #[test]
    fn songs_without_a_file_are_not_numbered() {
        let set = vec![Some(PathBuf::from("a")), None, Some(PathBuf::from("c"))];
        assert_eq!(number_in(&set, 0), Some(1));
        assert_eq!(number_in(&set, 1), None);
        assert_eq!(number_in(&set, 2), Some(2));
        assert_eq!(number_in(&set, 3), None);
    }

    #[test]
    fn modes_parse() {
        assert_eq!(Mode::parse(" Attach "), Some(Mode::Attach));
        assert_eq!(Mode::parse("spawn"), Some(Mode::Spawn));
        assert_eq!(Mode::parse("nope"), None);
    }

    #[test]
    fn urls_split() {
        assert_eq!(
            split_url("http://127.0.0.1:47811/mcp"),
            Some(("127.0.0.1:47811".into(), "/mcp".into()))
        );
    }

    #[test]
    fn bodies_read_plain_chunked_and_streamed() {
        let plain = b"HTTP/1.1 200 OK\r\ncontent-length: 7\r\n\r\n{\"a\":1}";
        assert_eq!(response_body(plain).unwrap(), "{\"a\":1}");
        let chunked = b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n3\r\n{\"a\r\n4\r\n\":1}\r\n0\r\n\r\n";
        assert_eq!(response_body(chunked).unwrap(), "{\"a\":1}");
        let sse = b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\r\nevent: message\ndata: {\"a\":1}\n\n";
        assert_eq!(response_body(sse).unwrap(), "{\"a\":1}");
        assert!(response_body(b"HTTP/1.1 401 Unauthorized\r\n\r\n").is_err());
    }

    #[test]
    fn answers_read() {
        let ok = json!({"result": {"content": [{"type": "text", "text": "{\"playing\":true}"}], "isError": false, "structuredContent": {"playing": true}}});
        assert_eq!(action_answer(&ok).unwrap(), json!({"playing": true}));
        let text = json!({"result": {"content": [{"type": "text", "text": "{\"x\":2}"}]}});
        assert_eq!(action_answer(&text).unwrap(), json!({"x": 2}));
        let bad =
            json!({"result": {"content": [{"type": "text", "text": "no song"}], "isError": true}});
        assert_eq!(action_answer(&bad).unwrap_err(), "no song");
    }
}
