//! A private engine: `daw-extension --engine` as `just engine` runs it,
//! headless, in a sandbox of its own so it never meets the owner's
//! engine or window, and always stopped (the guard's `Drop`), pass or
//! fail.
//!
//! Isolation, variable by variable:
//! - `FTS_DAW_ENGINE_DIR`: its own lock and control socket (else a second
//!   engine finds the owner's lock and exits);
//! - `XDG_RUNTIME_DIR`: its facade socket;
//! - `XDG_DATA_HOME`: the recent-songs list it writes on open;
//! - `FTS_ENGINES_DIR`: its registry file (`studio-<pid>.json`) and its
//!   `last-open-<name>.json` in the sandbox, never in the owner's
//!   `<config>/fts/engines`, where a window could find it;
//! - `STUDIO_INSTANCE`: its own `last-open-<name>.json`, so the owner's
//!   `--reopen` never opens the test's copy;
//! - `FTS_MCP_PORT=0`, `FTS_MCP_PRIVATE=1`: its MCP door on a free port,
//!   found in its registry file, and `session-mcp.json` left to the owner;
//! - `FTS_AUDIO_DEVICE=null`: no sound card, the callback at real time;
//! - `FTS_RENDER_WORKER=process`: renders in the engine, as the owner's;
//! - `OTEL_EXPORTER_OTLP_ENDPOINT`: the test's receiver, never the
//!   owner's collector.
//!
//! Shared on purpose, read only: `~/.config/signal` (Signal's libraries
//! and models), `~/.clap`, the sample libraries.

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::mcp::Mcp;

/// How long an engine has to come up and serve its door.
const START_WAIT: Duration = Duration::from_secs(90);
/// How long it has to stop when asked, before it is killed.
const STOP_WAIT: Duration = Duration::from_secs(20);

/// A running private engine.
pub struct Engine {
    child: Child,
    pub pid: u32,
    pub mcp: Mcp,
    pub mcp_url: String,
    /// Its output (the engine's log goes to stdout/stderr).
    pub log: PathBuf,
    engine_dir: PathBuf,
    registry: Option<PathBuf>,
    /// Sockets live here: kept short (a Unix socket path is at most 108
    /// bytes), so under `/tmp`.
    _sandbox: tempfile::TempDir,
}

impl Engine {
    /// Start `bin --engine`, exporting its spans to `otlp`, as instance
    /// `instance`; returns once its MCP door answers.
    ///
    /// # Errors
    ///
    /// It did not start, or did not serve its door in time (its log's
    /// tail is in the error).
    pub fn start(bin: &Path, otlp: &str, instance: &str) -> Result<Self, String> {
        let sandbox = tempfile::Builder::new()
            .prefix("ube-")
            .tempdir_in("/tmp")
            .map_err(|e| format!("a sandbox: {e}"))?;
        let root = sandbox.path();
        let (run, data, engine_dir) = (root.join("run"), root.join("data"), root.join("engine"));
        let engines = root.join("engines");
        for dir in [&run, &data, &engine_dir, &engines] {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let log = root.join("engine.out");
        let out = std::fs::File::create(&log).map_err(|e| e.to_string())?;
        let err = out.try_clone().map_err(|e| e.to_string())?;
        let mut command = Command::new(bin);
        command.arg("--engine");
        for var in [
            "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
            "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
            "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
            "OTEL_EXPORTER_OTLP_HEADERS",
            "OTEL_EXPORTER_OTLP_PROTOCOL",
            "FTS_MCP_TOKEN",
            "FTS_MCP",
            "FTS_DAW_OPEN",
            "FTS_ENGINE_SILENT",
            "STUDIO_ENGINE",
            "STUDIO_EXT_DEV",
            "FTS_ENGINES_DIR",
        ] {
            command.env_remove(var);
        }
        command
            .env("FTS_DAW_ENGINE_DIR", &engine_dir)
            .env("XDG_RUNTIME_DIR", &run)
            .env("XDG_DATA_HOME", &data)
            .env("FTS_ENGINES_DIR", &engines)
            .env("STUDIO_INSTANCE", instance)
            .env("FTS_MCP_PORT", "0")
            .env("FTS_MCP_PRIVATE", "1")
            .env("FTS_AUDIO_DEVICE", "null")
            .env("FTS_RENDER_WORKER", "process")
            .env("OTEL_EXPORTER_OTLP_ENDPOINT", otlp)
            .env("OTEL_BSP_SCHEDULE_DELAY", "100")
            .env("OTEL_BSP_MAX_QUEUE_SIZE", "65536")
            .env("OTEL_METRIC_EXPORT_INTERVAL", "600000")
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err);
        // It dies with the test: a test killed by its runner (a timeout)
        // runs no Drop, and an engine left behind holds a song and a core.
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt as _;
            // SAFETY: prctl is async-signal-safe; nothing else runs
            // between fork and exec here.
            unsafe {
                command.pre_exec(|| {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) == 0 {
                        Ok(())
                    } else {
                        Err(std::io::Error::last_os_error())
                    }
                });
            }
        }
        let child = command
            .spawn()
            .map_err(|e| format!("starting {}: {e}", bin.display()))?;
        let pid = child.id();
        let mut engine = Self {
            child,
            pid,
            mcp: Mcp::new("http://127.0.0.1:1/mcp")?,
            mcp_url: String::new(),
            log,
            engine_dir,
            registry: Some(engines.join(format!("studio-{pid}.json"))),
            _sandbox: sandbox,
        };
        let began = Instant::now();
        loop {
            if let Ok(Some(status)) = engine.child.try_wait() {
                return Err(format!(
                    "the engine exited ({status}):\n{}",
                    engine.tail(40)
                ));
            }
            if let Some(url) = engine.registry.as_deref().and_then(mcp_in) {
                let mcp = Mcp::new(&url)?;
                if mcp.ping() {
                    engine.mcp = mcp;
                    engine.mcp_url = url;
                    return Ok(engine);
                }
            }
            if began.elapsed() > START_WAIT {
                return Err(format!(
                    "the engine served no MCP door in {START_WAIT:?}:\n{}",
                    engine.tail(40)
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// The last `lines` lines of its output.
    #[must_use]
    pub fn tail(&self, lines: usize) -> String {
        let text = std::fs::read_to_string(&self.log).unwrap_or_default();
        let all: Vec<&str> = text.lines().collect();
        all[all.len().saturating_sub(lines)..].join("\n")
    }

    /// Ask it to stop on its control socket (`daw-extension --stop`'s
    /// request), and kill it if it has not gone in time. SIGTERM is not
    /// enough: it ends the engine's thread, not the process.
    pub fn stop(&mut self) {
        if matches!(self.child.try_wait(), Ok(Some(_))) {
            self.forget();
            return;
        }
        let asked = UnixStream::connect(self.engine_dir.join("control.sock")).and_then(|mut s| {
            s.set_read_timeout(Some(Duration::from_secs(5)))?;
            s.write_all(b"{\"stop\":true}\n")?;
            let mut line = String::new();
            BufReader::new(s).read_line(&mut line).map(|_| ())
        });
        let began = Instant::now();
        while asked.is_ok() && began.elapsed() < STOP_WAIT {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                self.forget();
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.forget();
    }

    /// Its registry file goes with it (a killed engine leaves it).
    fn forget(&mut self) {
        if let Some(file) = self.registry.take() {
            let _ = std::fs::remove_file(file);
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The MCP URL a registry file names.
fn mcp_in(file: &Path) -> Option<String> {
    let found: serde_json::Value = serde_json::from_slice(&std::fs::read(file).ok()?).ok()?;
    let url = found["mcp"].as_str()?;
    // Before the door is bound the file may name the port asked for (0).
    (!url.contains(":0/")).then(|| url.to_owned())
}
