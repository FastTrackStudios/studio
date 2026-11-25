//! The running app, reached through its control socket.
//!
//! Studio serves its Action List on a socket only this user can open,
//! `<runtime dir>/fts/studio.sock` (architect-control's endpoint), at the
//! same path every launch. So the bridge needs no port and no token: it
//! opens a connection per call, and a call made while Studio restarts
//! waits for the socket to come back ([`Socket::wait`]) instead of
//! failing. The socket's answers are whole (MCP's HTTP door cuts at
//! 100 KB), which is what lets the bridge make them compact itself.

use std::io::{BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// A failure as the socket says it: `{"code", "error", …}`.
pub type Failure = Value;

/// A failure of the bridge's own, in the socket's shape.
#[must_use]
pub fn failure(code: &str, error: impl Into<String>) -> Failure {
    json!({ "code": code, "error": error.into() })
}

/// What a call can be stopped through: a job's cancel.
#[derive(Debug, Default)]
pub struct Cancel {
    cancelled: AtomicBool,
    #[cfg(unix)]
    stream: Mutex<Option<std::os::unix::net::UnixStream>>,
}

impl Cancel {
    /// Stop it: the call waiting now gives up at once, and the next one
    /// is not made.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        #[cfg(unix)]
        let stream = self
            .stream
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(stream) = stream {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// One request to the app: `method` (`run`, `describe`, `actions`,
/// `batch`, `watch`, `hello`) with its params, its result or failure.
pub trait Upstream: Send + Sync {
    /// # Errors
    ///
    /// The app's failure, or `unavailable` when no app answers.
    fn call(&self, method: &str, params: &Value, cancel: &Cancel) -> Result<Value, Failure>;
}

/// `run` of action `id`.
///
/// # Errors
///
/// As [`Upstream::call`].
pub fn run(up: &dyn Upstream, id: &str, args: &Value, cancel: &Cancel) -> Result<Value, Failure> {
    up.call("run", &json!({ "id": id, "args": args }), cancel)
}

/// Where the app's socket is, in order: `--socket`, `STUDIO_MCP_SOCKET`,
/// then `<FTS_CONTROL_DIR or the runtime dir or the temp dir>/fts/<app>.sock`,
/// as architect-control puts it.
#[must_use]
pub fn socket_path(
    app: &str,
    explicit: Option<&Path>,
    env: &dyn Fn(&str) -> Option<String>,
) -> PathBuf {
    if let Some(p) = explicit {
        return p.to_path_buf();
    }
    let set = |k: &str| env(k).filter(|v| !v.is_empty());
    if let Some(p) = set("STUDIO_MCP_SOCKET") {
        return PathBuf::from(p);
    }
    let dir = set("FTS_CONTROL_DIR").map_or_else(
        || {
            set("XDG_RUNTIME_DIR")
                .map_or_else(std::env::temp_dir, PathBuf::from)
                .join("fts")
        },
        PathBuf::from,
    );
    dir.join(format!("{app}.sock"))
}

/// The control socket of a running app.
#[derive(Debug)]
pub struct Socket {
    path: PathBuf,
    /// How long a call waits for the app to come (back) up.
    wait: Duration,
    next: Mutex<u64>,
}

impl Socket {
    #[must_use]
    pub const fn new(path: PathBuf, wait: Duration) -> Self {
        Self {
            path,
            wait,
            next: Mutex::new(1),
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn connect(&self, cancel: &Cancel) -> Result<Conn, Failure> {
        let start = Instant::now();
        loop {
            match open(&self.path) {
                Ok(c) => return Ok(c),
                Err(e) => {
                    if cancel.is_cancelled() || start.elapsed() >= self.wait {
                        return Err(failure(
                            "unavailable",
                            format!(
                                "Studio is not answering on {} ({e}). Start it (apps/studio/docs/development.md: \
                                 `dx serve` in apps/studio, or the installed Studio); this bridge finds it again \
                                 by itself, no reconnect needed.",
                                self.path.display()
                            ),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
            }
        }
    }
}

#[cfg(unix)]
type Conn = std::os::unix::net::UnixStream;
#[cfg(not(unix))]
type Conn = std::fs::File;

#[cfg(unix)]
fn open(path: &Path) -> std::io::Result<Conn> {
    std::os::unix::net::UnixStream::connect(path)
}

#[cfg(not(unix))]
fn open(path: &Path) -> std::io::Result<Conn> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
}

impl Upstream for Socket {
    fn call(&self, method: &str, params: &Value, cancel: &Cancel) -> Result<Value, Failure> {
        if cancel.is_cancelled() {
            return Err(failure("cancelled", "the job was cancelled"));
        }
        let id = {
            let mut n = self.next.lock().unwrap_or_else(PoisonError::into_inner);
            *n = n.saturating_add(1);
            *n
        };
        let mut conn = self.connect(cancel)?;
        #[cfg(unix)]
        if let Ok(clone) = conn.try_clone() {
            *cancel.stream.lock().unwrap_or_else(PoisonError::into_inner) = Some(clone);
        }
        let gone = |e: std::io::Error| {
            if cancel.is_cancelled() {
                failure("cancelled", "the job was cancelled")
            } else {
                failure(
                    "unavailable",
                    format!("Studio went away during the call ({e}); call again once it is back"),
                )
            }
        };
        let mut line = json!({ "id": id, "method": method, "params": params }).to_string();
        line.push('\n');
        conn.write_all(line.as_bytes()).map_err(gone)?;
        conn.flush().map_err(gone)?;
        let mut reader = BufReader::new(conn);
        let mut answer = String::new();
        let n = reader.read_line(&mut answer).map_err(gone)?;
        if n == 0 {
            return Err(gone(std::io::Error::other("connection closed")));
        }
        let value: Value = serde_json::from_str(&answer)
            .map_err(|e| failure("protocol", format!("Studio's answer is not JSON: {e}")))?;
        if let Some(error) = value.get("error") {
            return Err(error.clone());
        }
        value
            .get("result")
            .cloned()
            .ok_or_else(|| failure("protocol", format!("not a reply: {value}")))
    }
}

/// A shared upstream.
pub type Shared = Arc<dyn Upstream>;
