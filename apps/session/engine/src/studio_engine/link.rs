//! A window and the engine it drives in another process: finding a running
//! `studio engine`, spawning one, and running the Action List on it.
//!
//! An engine says where it is in `$XDG_CONFIG_HOME/fts/engines/studio-<pid>.json`
//! ([`Found`], written by [`publish`] as it comes up, removed as it stops).
//! A desktop window, as it launches, picks how to have an engine
//! ([`Mode`], `FTS_ENGINE=spawn|attach`) and [`plan`]s from what is
//! running: attach to a live one, or spawn one ([`spawn`]: `studio engine`
//! in a process group of its own, so the audio outlives the window). There
//! is no engine in a desktop window's process. Linked ([`set_linked`]), the
//! window's actions that are not its own run on the engine ([`run`]), and
//! everything it shows comes over its link (`crate::view_client`).
//!
//! The decisions ([`Mode::parse`], [`plan`], [`newest_first`], [`changed_songs`])
//! are pure; the rest is the file system, a socket and a child process.

use std::path::{Path, PathBuf};
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
    /// Which build it is ([`build_id`]): a window of another build (an
    /// update, a rebuild) starts one of its own in its place.
    #[serde(default)]
    pub build: String,
}

/// This program's build: its executable's size and modification time — what
/// tells a window an engine of the same build from an older one.
#[must_use]
pub fn build_id() -> String {
    std::env::current_exe()
        .and_then(std::fs::metadata)
        .map(|m| {
            let at = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(web_time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            format!("{}-{at}", m.len())
        })
        .unwrap_or_default()
}

/// How a desktop window has its engine — always in another process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// A running engine if there is one, else one spawned for it.
    Spawn,
    /// A running engine only: the window waits for one (started by hand,
    /// under a debugger).
    Attach,
}

impl Mode {
    /// `FTS_ENGINE`'s spelling of a mode.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "spawn" => Some(Self::Spawn),
            "attach" => Some(Self::Attach),
            _ => None,
        }
    }

    /// The mode `FTS_ENGINE` asks for, else [`Mode::Spawn`]. There is no
    /// engine in the window's process on the desktop: the window is a
    /// client of `studio engine` (`docs/design/engine-boundary.md`).
    #[must_use]
    pub fn from_env() -> Self {
        let asked = std::env::var("FTS_ENGINE").ok();
        match asked.as_deref().map(Self::parse) {
            Some(Some(mode)) => mode,
            Some(None) => {
                tracing::warn!(
                    engine.asked = asked.as_deref().unwrap_or_default(),
                    "engine: FTS_ENGINE is spawn or attach; spawning"
                );
                Self::Spawn
            }
            None => Self::Spawn,
        }
    }
}

/// What a launching window does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    Attach(Found),
    /// One of another build runs: stopped, and one of this build started
    /// in its place, opening what it had open.
    Replace(Found),
    Spawn,
    /// None running, and none to be spawned: wait for one.
    Wait,
}

/// Engines newest first: a window attaches to the one most recently
/// started.
#[must_use]
pub fn newest_first(mut found: Vec<(Found, std::time::SystemTime)>) -> Vec<Found> {
    found.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.pid.cmp(&a.0.pid)));
    found.into_iter().map(|(f, _)| f).collect()
}

/// What to do, in `mode`, with `found` running (newest first), `alive`
/// saying which of them answer and `ours` which are of this window's build.
#[must_use]
pub fn plan(
    mode: Mode,
    found: &[Found],
    alive: impl Fn(&Found) -> bool,
    ours: impl Fn(&Found) -> bool,
) -> Plan {
    match (found.iter().find(|f| alive(f)), mode) {
        (Some(running), Mode::Spawn) if !ours(running) => Plan::Replace(running.clone()),
        (Some(running), _) => Plan::Attach(running.clone()),
        (None, Mode::Spawn) => Plan::Spawn,
        (None, Mode::Attach) => Plan::Wait,
    }
}

/// Whether `found` is of the build this window would start: always, when
/// the engine is a `studio` CLI of its own (`FTS_STUDIO_BIN`).
#[must_use]
pub fn of_this_build(found: &Found) -> bool {
    std::env::var_os("FTS_STUDIO_BIN").is_some_and(|b| !b.is_empty()) || found.build == build_id()
}

/// Stop the engine `found` (SIGTERM: it stops its audio, says it is gone
/// and exits), waiting at most `wait` for it to go.
///
/// # Errors
///
/// It could not be signalled, or it did not go in time.
pub fn stop(found: &Found, wait: Duration) -> Result<(), String> {
    let sent = std::process::Command::new("kill")
        .args(["-TERM", &found.pid.to_string()])
        .status()
        .map_err(|e| e.to_string())?;
    if !sent.success() {
        return Err(format!(
            "the engine (pid {}) could not be stopped",
            found.pid
        ));
    }
    let began = web_time::Instant::now();
    while began.elapsed() < wait {
        if !alive(found) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!("the engine (pid {}) did not stop", found.pid))
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

/// How to start an engine: a program, and the arguments before the
/// engine's own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineCommand {
    pub bin: PathBuf,
    pub args: Vec<String>,
}

/// The engine to start: `FTS_STUDIO_BIN engine` when that names a `studio`
/// CLI, else this very program as its own engine (`--session-engine`) — the
/// same build as the window, so the two always speak one protocol.
#[must_use]
pub fn engine_command() -> Option<EngineCommand> {
    if let Some(bin) = std::env::var_os("FTS_STUDIO_BIN").filter(|b| !b.is_empty()) {
        return Some(EngineCommand {
            bin: PathBuf::from(bin),
            args: vec!["engine".to_owned()],
        });
    }
    let exe = std::env::current_exe().ok()?;
    Some(EngineCommand {
        bin: exe,
        args: vec!["--session-engine".to_owned()],
    })
}

/// Where a spawned engine logs.
#[must_use]
pub fn log_file() -> Option<PathBuf> {
    Some(dirs::cache_dir()?.join("fts").join("studio-engine.log"))
}

/// Start an engine with `command` (and `args` of its own), in a process
/// group of its own (a window that crashes, or a Ctrl-C to it, leaves the
/// engine playing), its output to [`log_file`]; wait at most `wait` for it
/// to say where it is.
///
/// # Errors
///
/// It could not be started, or it did not come up in time.
pub fn spawn(command: &EngineCommand, args: &[&str], wait: Duration) -> Result<Found, String> {
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
    let bin = &command.bin;
    let mut child = std::process::Command::new(bin)
        .args(&command.args)
        .args(args)
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

/// Have an engine as `mode` says: one running attached to, else one
/// spawned (or, attaching only, waited for up to `wait`).
///
/// # Errors
///
/// No `studio` to spawn, it did not come up, or none came to attach to.
pub fn choose(mode: Mode, wait: Duration) -> Result<Found, String> {
    let span = tracing::info_span!("engine.link", engine.mode = ?mode, engine.plan = tracing::field::Empty);
    let _in = span.enter();
    let decided = plan(mode, &found(), alive, of_this_build);
    span.record("engine.plan", format!("{decided:?}").as_str());
    match decided {
        Plan::Attach(running) => Ok(running),
        Plan::Replace(old) => {
            tracing::warn!(engine.pid = old.pid, engine.build = %old.build, "engine: one of another build runs; this build's in its place");
            stop(&old, Duration::from_secs(10))?;
            let command = engine_command().ok_or("no program to run the engine with")?;
            spawn(&command, &["--reopen"], wait)
        }
        Plan::Spawn => {
            let command = engine_command().ok_or("no program to run the engine with")?;
            spawn(&command, &[], wait)
        }
        Plan::Wait => {
            let began = web_time::Instant::now();
            while began.elapsed() < wait {
                if let Some(up) = found().into_iter().find(alive) {
                    return Ok(up);
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            Err("no engine came up to attach to (FTS_ENGINE=attach)".to_owned())
        }
    }
}

// ── Linked ───────────────────────────────────────────────────────────

/// The engine linked to now (a relink replaces it).
static CURRENT: std::sync::RwLock<Option<Found>> = std::sync::RwLock::new(None);

/// This window drives `found` from now on.
pub fn set_linked(found: Found) {
    if let Ok(mut now) = CURRENT.write() {
        *now = Some(found);
    }
}

/// The engine this window drives in another process, if it does.
#[must_use]
pub fn linked() -> Option<Found> {
    CURRENT.read().ok().and_then(|f| f.clone())
}

/// Run `id` with `args` (JSON) on the engine's Action List, over the
/// window's link to it (`crate::view_client`); what it said.
///
/// # Errors
///
/// Not linked, the engine did not answer, or the action refused.
pub fn run(id: &str, args: &[u8]) -> Result<String, String> {
    crate::view_client::run(id, args)
}

// ── Kept linked ─────────────────────────────────────────────────────

/// What a window said about its engine coming back: shown once by the
/// window (`take_notice`).
static NOTICE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// What there is to tell the person about the engine since last asked
/// (it stopped and was started again), once.
#[must_use]
pub fn take_notice() -> Option<String> {
    NOTICE.lock().ok().and_then(|mut n| n.take())
}

fn notice(text: String) {
    if let Ok(mut slot) = NOTICE.lock() {
        *slot = Some(text);
    }
}

/// Keep this window linked for as long as the process lives: when its
/// engine goes away (killed, crashed), find another running one or spawn
/// one — asked to open again what the last one had open — attach to it
/// with `attach` (the facade and the view link re-made), and say so; when
/// the link alone dropped (the engine still answers), attach to it again.
/// `mode` as at launch. Once per process.
pub fn keep_linked(mode: Mode, attach: fn(&Found) -> Result<(), String>) {
    static KEEPING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if KEEPING.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("engine-keep".into())
        .spawn(move || {
            // Relinks close together back off: an engine that keeps
            // dropping the link (one of another build) is not hammered.
            let mut pause = Duration::from_millis(250);
            loop {
                std::thread::sleep(Duration::from_millis(250));
                if crate::view_client::state() != crate::view_client::LinkState::Down {
                    pause = Duration::from_millis(250);
                    continue;
                }
                relink(mode, attach);
                std::thread::sleep(pause);
                pause = (pause * 2).min(Duration::from_secs(10));
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "engine: no thread to keep the window linked");
    }
}

/// The link went down: the same engine again while it answers, else
/// another — until one is linked.
fn relink(mode: Mode, attach: fn(&Found) -> Result<(), String>) {
    let was = linked();
    let gone = was.as_ref().map(|f| f.pid);
    let mut wait = Duration::from_millis(250);
    loop {
        let span = tracing::info_span!(
            "engine.relink",
            engine.gone = gone,
            engine.pid = tracing::field::Empty,
            engine.outcome = tracing::field::Empty
        );
        let _in = span.enter();
        // The engine still answers: only the link dropped.
        if let Some(same) = was.as_ref().filter(|f| alive(f)) {
            match attach(same) {
                Ok(()) => {
                    span.record("engine.pid", same.pid);
                    span.record("engine.outcome", "reattached");
                    return;
                }
                Err(why) => {
                    span.record("engine.outcome", "retry");
                    tracing::warn!(error = %why, "engine: it answers but did not take the window again");
                    std::thread::sleep(wait);
                    wait = (wait * 2).min(Duration::from_secs(5));
                    continue;
                }
            }
        }
        // Gone. Dead ones say where they were until their file is swept:
        // never one of those again.
        let still_up = |f: &Found| Some(f.pid) != gone && alive(f);
        let found = match plan(mode, &found(), still_up, of_this_build) {
            Plan::Attach(running) => Ok(running),
            Plan::Replace(old) => stop(&old, Duration::from_secs(10)).and_then(|()| {
                engine_command()
                    .ok_or_else(|| "no program to run the engine with".to_owned())
                    .and_then(|command| spawn(&command, &["--reopen"], Duration::from_secs(30)))
            }),
            Plan::Spawn => engine_command()
                .ok_or_else(|| "no program to run the engine with".to_owned())
                .and_then(|command| spawn(&command, &["--reopen"], Duration::from_secs(30))),
            Plan::Wait => Err("waiting for an engine to attach to".to_owned()),
        };
        match found.and_then(|f| attach(&f).map(|()| f)) {
            Ok(up) => {
                span.record("engine.pid", up.pid);
                span.record("engine.outcome", "relinked");
                tracing::warn!(
                    engine.pid = up.pid,
                    "engine: it stopped; this window is on a new one"
                );
                notice(format!(
                    "The engine stopped (pid {}); Studio started it again (pid {}) and opened what it had open.",
                    gone.unwrap_or_default(),
                    up.pid
                ));
                set_linked(up);
                return;
            }
            Err(why) => {
                span.record("engine.outcome", "retry");
                tracing::warn!(error = %why, "engine: not linked yet; trying again");
                std::thread::sleep(wait);
                wait = (wait * 2).min(Duration::from_secs(5));
            }
        }
    }
}

// ── What the engine last opened ──────────────────────────────────────

/// Where an engine keeps what it last opened (`id` and its arguments), for
/// one started in its place to open it again.
fn last_open_file() -> Option<PathBuf> {
    Some(dir()?.join("last-open.json"))
}

/// The engine opened something with `id` and `args`: kept, for an engine
/// started after it dies (`--reopen`).
pub fn remember_open(id: &str, args: &serde_json::Value) {
    let Some(file) = last_open_file() else { return };
    let kept = serde_json::json!({ "id": id, "args": args });
    let written = file
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&file, kept.to_string()));
    if let Err(e) = written {
        tracing::warn!(error = %e, "engine: what it opened could not be kept");
    }
}

/// Open what the last engine had open ([`remember_open`]), on the Action
/// List of the engine in this process.
pub fn reopen_last() {
    let Some(kept) = last_open_file()
        .and_then(|f| std::fs::read(f).ok())
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
    else {
        return;
    };
    let (Some(id), Some(args)) = (kept["id"].as_str(), kept.get("args")) else {
        return;
    };
    let ran = crate::actions::list().run_json(id, args.to_string().as_bytes());
    match ran {
        Ok(_) => tracing::info!(
            engine.reopen = id,
            "engine: opened what the last one had open"
        ),
        Err(e) => {
            tracing::warn!(engine.reopen = id, error = %e, "engine: what the last one had open did not open")
        }
    }
}

// ── The set, followed ────────────────────────────────────────────────

/// Whether the songs a window shows are the engine's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongsChanged {
    /// The same songs, in the same order: follow the rest in place.
    Same,
    /// Others (another set, a song arrived or gone, reordered): read them.
    Read,
}

/// Whether `shown` (the window's songs' projects) are `engine`'s.
#[must_use]
pub fn changed_songs(shown: &[String], engine: &[String]) -> SongsChanged {
    if shown == engine {
        SongsChanged::Same
    } else {
        SongsChanged::Read
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
            build: "b1".to_owned(),
        }
    }

    #[test]
    fn fts_engine_names_the_two_modes_and_nothing_runs_in_the_window() {
        assert_eq!(Mode::parse("spawn"), Some(Mode::Spawn));
        assert_eq!(Mode::parse(" Attach "), Some(Mode::Attach));
        assert_eq!(Mode::parse("inprocess"), None);
        assert_eq!(Mode::parse("remote"), None);
    }

    #[test]
    fn a_running_engine_is_attached_to_whatever_the_mode() {
        let running = [engine(7), engine(5)];
        let alive = |f: &Found| f.pid == 5;
        let ours = |f: &Found| f.build == "b1";
        assert_eq!(
            plan(Mode::Spawn, &running, alive, ours),
            Plan::Attach(engine(5))
        );
        assert_eq!(
            plan(Mode::Attach, &running, alive, ours),
            Plan::Attach(engine(5))
        );
    }

    #[test]
    fn an_engine_of_another_build_is_replaced_unless_only_attaching() {
        let running = [engine(5)];
        let theirs = |_: &Found| false;
        assert_eq!(
            plan(Mode::Spawn, &running, |_| true, theirs),
            Plan::Replace(engine(5))
        );
        assert_eq!(
            plan(Mode::Attach, &running, |_| true, theirs),
            Plan::Attach(engine(5))
        );
    }

    #[test]
    fn with_none_running_spawn_spawns_and_attach_waits() {
        let dead = [engine(7)];
        let ours = |_: &Found| true;
        assert_eq!(plan(Mode::Spawn, &dead, |_| false, ours), Plan::Spawn);
        assert_eq!(plan(Mode::Attach, &dead, |_| false, ours), Plan::Wait);
        assert_eq!(plan(Mode::Spawn, &[], |_| true, ours), Plan::Spawn);
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
    fn the_window_reads_songs_only_when_they_are_not_the_ones_it_shows() {
        let songs = |p: &[&str]| p.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            changed_songs(&songs(&["a", "b"]), &songs(&["a", "b"])),
            SongsChanged::Same
        );
        assert_eq!(
            changed_songs(&songs(&["a"]), &songs(&["a", "b"])),
            SongsChanged::Read
        );
        assert_eq!(
            changed_songs(&songs(&["b", "a"]), &songs(&["a", "b"])),
            SongsChanged::Read
        );
        assert_eq!(changed_songs(&[], &[]), SongsChanged::Same);
    }
}
