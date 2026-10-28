//! A window and the engine it drives in another process: finding a running
//! `studio engine`, spawning one, and running the Action List on it.
//!
//! An engine says where it is in `$XDG_CONFIG_HOME/fts/engines/studio-<pid>.json`
//! ([`Found`], written by [`publish`] as it comes up, removed as it stops).
//! A desktop window, as it launches, picks how to have an engine
//! ([`Mode`], `FTS_ENGINE=inprocess|spawn|attach`) and [`plan`]s from what
//! is running: attach to a live one, spawn one ([`spawn`]: `studio engine`
//! in a process group of its own, so the audio outlives the window), or run
//! the engine in this process as before. Linked ([`set_linked`]), the
//! window's actions that are not its own run on the engine ([`run`]), the
//! set it shows follows the engine's ([`follow`]), and its picks and opens
//! go to the engine (`super::set`).
//!
//! The decisions ([`Mode::parse`], [`plan`], [`newest_first`], [`Changed`])
//! are pure; the rest is the file system, a socket, a child process and the
//! engine's MCP endpoint.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

/// Where a running engine is, for a window or the CLI to find it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Found {
    pub pid: u32,
    /// The daw facade, over vox (the lanes REAPER's bridge serves).
    pub socket: PathBuf,
    /// The Action List over MCP (`http://127.0.0.1:47811/mcp`).
    pub mcp: String,
    /// The facade over WebSocket, when the engine serves one.
    pub ws: Option<String>,
}

/// How a window has its engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// In this process, as a window always had it.
    InProcess,
    /// A running engine if there is one, else one spawned for it.
    Spawn,
    /// A running engine only; in this process when there is none.
    Attach,
}

impl Mode {
    /// `FTS_ENGINE`'s spelling of a mode.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "inprocess" | "in-process" | "local" => Some(Self::InProcess),
            "spawn" => Some(Self::Spawn),
            "attach" => Some(Self::Attach),
            _ => None,
        }
    }

    /// The mode `FTS_ENGINE` asks for, else this platform's: in process on
    /// iOS and in a browser (no helper process there), and — until the
    /// window draws everything it draws in process from an engine in
    /// another (`docs/design/engine-boundary.md` step 5) — on the desktop.
    #[must_use]
    pub fn from_env() -> Self {
        std::env::var("FTS_ENGINE")
            .ok()
            .and_then(|v| Self::parse(&v))
            .unwrap_or(Self::InProcess)
    }
}

/// What a launching window does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    InProcess,
    Attach(Found),
    Spawn,
}

/// Engines newest first: a window attaches to the one most recently
/// started.
#[must_use]
pub fn newest_first(mut found: Vec<(Found, std::time::SystemTime)>) -> Vec<Found> {
    found.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.pid.cmp(&a.0.pid)));
    found.into_iter().map(|(f, _)| f).collect()
}

/// What to do, in `mode`, with `found` running (newest first) and
/// `alive` saying which of them answer.
#[must_use]
pub fn plan(mode: Mode, found: &[Found], alive: impl Fn(&Found) -> bool) -> Plan {
    if mode == Mode::InProcess {
        return Plan::InProcess;
    }
    match (found.iter().find(|f| alive(f)), mode) {
        (Some(running), _) => Plan::Attach(running.clone()),
        (None, Mode::Spawn) => Plan::Spawn,
        (None, _) => Plan::InProcess,
    }
}

// ── Where engines say they are ───────────────────────────────────────

/// Where engines say where they are.
#[must_use]
pub fn dir() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("fts").join("engines"))
}

/// `found`, written where engines are looked for; the file, or `None` when
/// it could not be written (said in the log — the engine serves anyway).
pub fn publish(found: &Found) -> Option<PathBuf> {
    let dir = dir()?;
    let file = dir.join(format!("studio-{}.json", found.pid));
    let written = std::fs::create_dir_all(&dir)
        .map_err(|e| e.to_string())
        .and_then(|()| serde_json::to_vec_pretty(found).map_err(|e| e.to_string()))
        .and_then(|bytes| std::fs::write(&file, bytes).map_err(|e| e.to_string()));
    match written {
        Ok(()) => Some(file),
        Err(e) => {
            tracing::warn!(error = %e, "engine: where it is could not be written");
            None
        }
    }
}

/// Every engine that has said where it is, newest first (dead ones too:
/// [`alive`] tells them apart).
#[must_use]
pub fn found() -> Vec<Found> {
    let Some(entries) = dir().and_then(|d| std::fs::read_dir(d).ok()) else {
        return Vec::new();
    };
    let read = entries.flatten().filter_map(|entry| {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            return None;
        }
        let when = entry.metadata().and_then(|m| m.modified()).ok()?;
        let found: Found = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
        Some((found, when))
    });
    newest_first(read.collect())
}

/// Whether `found` answers on its socket.
#[must_use]
pub fn alive(found: &Found) -> bool {
    std::os::unix::net::UnixStream::connect(&found.socket).is_ok()
}

// ── Spawning one ─────────────────────────────────────────────────────

/// The `studio` CLI to run `studio engine` with: `FTS_STUDIO_BIN`, else a
/// `studio` beside this executable, else one on `PATH`.
#[must_use]
pub fn studio_bin() -> Option<PathBuf> {
    if let Some(bin) = std::env::var_os("FTS_STUDIO_BIN").filter(|b| !b.is_empty()) {
        return Some(PathBuf::from(bin));
    }
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("studio")))
        .filter(|p| p.is_file());
    beside.or_else(|| {
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|d| d.join("studio"))
                .find(|p| p.is_file())
        })
    })
}

/// Where a spawned engine logs.
#[must_use]
pub fn log_file() -> Option<PathBuf> {
    Some(dirs::cache_dir()?.join("fts").join("studio-engine.log"))
}

/// Start `studio engine` with `bin`, in a process group of its own (a
/// window that crashes, or a Ctrl-C to it, leaves the engine playing), its
/// output to [`log_file`]; wait at most `wait` for it to say where it is.
///
/// # Errors
///
/// It could not be started, or it did not come up in time.
pub fn spawn(bin: &Path, wait: Duration) -> Result<Found, String> {
    use std::os::unix::process::CommandExt as _;
    let log = log_file().ok_or("no cache directory for the engine's log")?;
    if let Some(parent) = log.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .map_err(|e| format!("{}: {e}", log.display()))?;
    let err = out.try_clone().map_err(|e| e.to_string())?;
    let mut child = std::process::Command::new(bin)
        .arg("engine")
        .stdin(std::process::Stdio::null())
        .stdout(out)
        .stderr(err)
        .process_group(0)
        .spawn()
        .map_err(|e| format!("{}: {e}", bin.display()))?;
    let pid = child.id();
    tracing::info!(engine.pid = pid, engine.bin = %bin.display(), engine.log = %log.display(), "engine: spawned");
    // Reaped when it exits, so it never lingers as a zombie of this window.
    let _ = std::thread::Builder::new()
        .name("engine-child".into())
        .spawn(move || {
            let _ = child.wait();
        });
    let began = web_time::Instant::now();
    while began.elapsed() < wait {
        if let Some(up) = found().into_iter().find(|f| f.pid == pid && alive(f)) {
            return Ok(up);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!(
        "the engine (pid {pid}) did not come up within {} s — see {}",
        wait.as_secs(),
        log.display()
    ))
}

/// Have an engine as `mode` says: the one attached to or spawned, or
/// `None` for one in this process.
#[must_use]
pub fn choose(mode: Mode) -> Option<Found> {
    let span = tracing::info_span!("engine.link", engine.mode = ?mode, engine.plan = tracing::field::Empty);
    let _in = span.enter();
    let decided = plan(mode, &found(), alive);
    span.record("engine.plan", format!("{decided:?}").as_str());
    match decided {
        Plan::InProcess => None,
        Plan::Attach(running) => Some(running),
        Plan::Spawn => {
            let Some(bin) = studio_bin() else {
                tracing::warn!(
                    "engine: no `studio` to spawn (FTS_STUDIO_BIN); in this process instead"
                );
                return None;
            };
            spawn(&bin, Duration::from_secs(30))
                .inspect_err(|why| tracing::warn!(error = %why, "engine: not spawned; in this process instead"))
                .ok()
        }
    }
}

// ── Linked ───────────────────────────────────────────────────────────

static LINKED: OnceLock<Found> = OnceLock::new();

/// This window drives `found` from now on (once per process).
pub fn set_linked(found: Found) {
    let _ = LINKED.set(found);
}

/// The engine this window drives in another process, if it does.
#[must_use]
pub fn linked() -> Option<&'static Found> {
    LINKED.get()
}

/// Run `id` with `args` (JSON) on the linked engine's Action List; what it
/// said. Waits at most 60 s (an open, a save).
///
/// # Errors
///
/// Not linked, the engine did not answer, or the action refused.
pub fn run(id: &str, args: &[u8]) -> Result<String, String> {
    let engine = linked().ok_or("not linked to an engine")?;
    let args: serde_json::Value = if args.iter().all(u8::is_ascii_whitespace) {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(args).map_err(|e| format!("{id}: {e}"))?
    };
    call(&engine.mcp, id, args)
}

/// One `run` on the MCP endpoint at `url`.
fn call(url: &str, id: &str, args: serde_json::Value) -> Result<String, String> {
    let mut params = serde_json::json!({ "id": id });
    if !args.is_null() {
        params["args"] = args;
    }
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "run", "arguments": params },
    });
    let url = url.to_owned();
    let rt = crate::open::engine_runtime().map_err(|e| e.to_string())?;
    let answer: serde_json::Value = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                rt.block_on(async {
                    let client = reqwest::Client::builder()
                        .timeout(Duration::from_secs(60))
                        .build()
                        .map_err(|e| e.to_string())?;
                    let mut request = client
                        .post(&url)
                        .header("content-type", "application/json")
                        .header("accept", "application/json, text/event-stream")
                        .body(body.to_string());
                    if let Some(token) = std::env::var("FTS_MCP_TOKEN")
                        .ok()
                        .filter(|t| !t.is_empty())
                    {
                        request = request.bearer_auth(token);
                    }
                    let text = request
                        .send()
                        .await
                        .map_err(|e| format!("{url}: {e}"))?
                        .text()
                        .await
                        .map_err(|e| e.to_string())?;
                    serde_json::from_str::<serde_json::Value>(&text).map_err(|e| e.to_string())
                })
            })
            .join()
            .unwrap_or_else(|_| Err("the call panicked".to_owned()))
    })?;
    if let Some(error) = answer.get("error") {
        return Err(format!("{id}: {error}"));
    }
    let result = answer.get("result").cloned().unwrap_or_default();
    let said: Vec<&str> = result
        .get("content")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.get("text").and_then(serde_json::Value::as_str))
        .collect();
    let said = said.join("\n");
    if result.get("isError").and_then(serde_json::Value::as_bool) == Some(true) {
        return Err(format!("{id}: {said}"));
    }
    Ok(said)
}

// ── The set, followed ────────────────────────────────────────────────

/// The engine's set as a window follows it: its songs' projects, in order,
/// and which is current.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    pub projects: Vec<String>,
    pub titles: Vec<String>,
    pub current: Option<usize>,
}

/// What changed between the set a window shows and the engine's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Changed {
    Nothing,
    /// Another set (or none): read it again.
    Set,
    /// Same songs, another current one.
    Current(usize),
}

/// What changed from `shown` to `now`.
#[must_use]
pub fn changed(shown: &Shown, now: &Shown) -> Changed {
    if shown.projects != now.projects {
        return Changed::Set;
    }
    match now.current {
        Some(at) if shown.current != Some(at) => Changed::Current(at),
        _ => Changed::Nothing,
    }
}

/// The linked engine's set, as `SESSION_SONGS` lists it (open songs only).
///
/// # Errors
///
/// The engine did not answer.
pub fn engine_set() -> Result<Shown, String> {
    let said = run("SESSION_SONGS", b"")?;
    let rows: Vec<serde_json::Value> = serde_json::from_str(&said).map_err(|e| e.to_string())?;
    let mut shown = Shown::default();
    for row in rows.iter().filter(|r| r["state"] == "open") {
        if row["current"].as_bool() == Some(true) {
            shown.current = Some(shown.projects.len());
        }
        shown
            .projects
            .push(row["project"].as_str().unwrap_or_default().to_owned());
        shown
            .titles
            .push(row["title"].as_str().unwrap_or_default().to_owned());
    }
    Ok(shown)
}

/// Follow the linked engine's set, on a thread of its own, for as long as
/// the process lives: a new set read in and shown, another current song
/// marked current, no set closed. `read` reads a set from the engine (its
/// songs, read back through the facade) — the window's
/// (`crate::setlist::Setlist::from_engine`).
pub fn follow(read: fn(&Shown) -> eyre::Result<crate::setlist::Setlist>) {
    static FOLLOWING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if FOLLOWING.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("engine-follow".into())
        .spawn(move || {
            let mut shown = Shown::default();
            loop {
                std::thread::sleep(Duration::from_millis(400));
                let Ok(now) = engine_set() else {
                    continue;
                };
                match changed(&shown, &now) {
                    Changed::Nothing => {}
                    Changed::Set if now.projects.is_empty() => {
                        super::set::close();
                        shown = now;
                    }
                    Changed::Set => match read(&now) {
                        Ok(setlist) => {
                            tracing::info!(
                                set.songs = now.projects.len(),
                                "engine: its set, shown"
                            );
                            super::set::install(setlist);
                            shown = now;
                        }
                        Err(e) => tracing::warn!(error = %e, "engine: its set could not be read"),
                    },
                    Changed::Current(at) => {
                        tracing::info!(set.current = at, "engine: its current song, shown");
                        super::set::mark_current(at, 0.0);
                        shown.current = Some(at);
                    }
                }
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "engine: no thread to follow its set");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(pid: u32) -> Found {
        Found {
            pid,
            socket: PathBuf::from(format!("/run/user/1000/fts/engine-{pid}.sock")),
            mcp: "http://127.0.0.1:47811/mcp".to_owned(),
            ws: None,
        }
    }

    #[test]
    fn fts_engine_names_the_three_modes() {
        assert_eq!(Mode::parse("spawn"), Some(Mode::Spawn));
        assert_eq!(Mode::parse(" Attach "), Some(Mode::Attach));
        assert_eq!(Mode::parse("inprocess"), Some(Mode::InProcess));
        assert_eq!(Mode::parse("in-process"), Some(Mode::InProcess));
        assert_eq!(Mode::parse("remote"), None);
    }

    #[test]
    fn a_running_engine_is_attached_to_whatever_the_mode_but_in_process() {
        let running = [engine(7), engine(5)];
        let alive = |f: &Found| f.pid == 5;
        assert_eq!(plan(Mode::Spawn, &running, alive), Plan::Attach(engine(5)));
        assert_eq!(plan(Mode::Attach, &running, alive), Plan::Attach(engine(5)));
        assert_eq!(plan(Mode::InProcess, &running, alive), Plan::InProcess);
    }

    #[test]
    fn with_none_running_spawn_spawns_and_attach_stays_in_process() {
        let dead = [engine(7)];
        assert_eq!(plan(Mode::Spawn, &dead, |_| false), Plan::Spawn);
        assert_eq!(plan(Mode::Attach, &dead, |_| false), Plan::InProcess);
        assert_eq!(plan(Mode::Spawn, &[], |_| true), Plan::Spawn);
    }

    #[test]
    fn the_newest_engine_comes_first() {
        let at = |s: u64| web_time::UNIX_EPOCH + Duration::from_secs(s);
        let order = newest_first(vec![
            (engine(1), at(10)),
            (engine(2), at(30)),
            (engine(3), at(20)),
        ]);
        let pids: Vec<u32> = order.iter().map(|f| f.pid).collect();
        assert_eq!(pids, [2, 3, 1]);
    }

    #[test]
    fn a_found_file_reads_back_as_written() {
        let text = serde_json::to_string(&engine(42)).unwrap_or_default();
        let back: Option<Found> = serde_json::from_str(&text).ok();
        assert_eq!(back, Some(engine(42)));
    }

    #[test]
    fn the_window_reads_a_new_set_and_follows_a_new_current_song() {
        let shown = Shown {
            projects: vec!["a".into(), "b".into()],
            titles: vec!["A".into(), "B".into()],
            current: Some(0),
        };
        assert_eq!(changed(&shown, &shown), Changed::Nothing);
        let next = Shown {
            current: Some(1),
            ..shown.clone()
        };
        assert_eq!(changed(&shown, &next), Changed::Current(1));
        let other = Shown {
            projects: vec!["c".into()],
            titles: vec!["C".into()],
            current: Some(0),
        };
        assert_eq!(changed(&shown, &other), Changed::Set);
        assert_eq!(
            changed(&Shown::default(), &Shown::default()),
            Changed::Nothing
        );
    }
}
