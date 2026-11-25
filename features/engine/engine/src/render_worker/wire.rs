//! The line between the engine and its render worker: one Unix socket,
//! one JSON value per line, bytes (a Loro snapshot or update, a plugin
//! state blob) as base64 inside it.
//!
//! The engine asks, one request at a time:
//!
//! - `{"op": "song", "key", "name", "media_dir", "snapshot"}`: build this
//!   song (a Loro snapshot of its doc), replacing any other.
//! - `{"op": "update", "update"}`: the doc's changes since the version the
//!   worker last answered; the worker applies them to its song.
//! - `{"op": "run", "id", "args"}`: run an action on the song.
//! - `{"op": "ping"}`, and `{"op": "fault", "kind", "plugin"}` (tests
//!   only: the worker crashes as a plugin would; refused unless the worker
//!   was started with faults allowed).
//!
//! While a request runs the worker may ask back `{"need": "blob", "name"}`
//! (a plugin state the doc names that it does not hold) and is answered
//! `{"blob": <base64> | null}`. A request ends with `{"done": {"ok": …}}`
//! or `{"done": {"err": "…"}}`. The first line the worker writes is
//! `{"hello": {"pid"}}`.

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixStream;

use base64::Engine as _;
use serde_json::Value;

/// One end of the line.
pub struct Conn {
    read: BufReader<UnixStream>,
    write: UnixStream,
}

impl Conn {
    /// # Errors
    /// The socket could not be cloned.
    pub fn new(stream: UnixStream) -> std::io::Result<Self> {
        Ok(Self {
            write: stream.try_clone()?,
            read: BufReader::new(stream),
        })
    }

    /// # Errors
    /// The other end is gone.
    pub fn send(&mut self, message: &Value) -> std::io::Result<()> {
        let mut line = serde_json::to_vec(message).map_err(std::io::Error::other)?;
        line.push(b'\n');
        self.write.write_all(&line)?;
        self.write.flush()
    }

    /// The next message.
    ///
    /// # Errors
    /// The other end is gone ([`std::io::ErrorKind::UnexpectedEof`]), the
    /// read timed out, or it sent something that is not JSON.
    pub fn recv(&mut self) -> std::io::Result<Value> {
        let mut line = String::new();
        if self.read.read_line(&mut line)? == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        serde_json::from_str(&line).map_err(std::io::Error::other)
    }

    /// How long a read may wait (`None`: for ever).
    ///
    /// # Errors
    /// The socket refused it.
    pub fn set_timeout(&self, wait: Option<std::time::Duration>) -> std::io::Result<()> {
        self.read.get_ref().set_read_timeout(wait)
    }
}

#[must_use]
pub fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// # Errors
/// It is not base64.
pub fn unb64(text: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .map_err(|e| e.to_string())
}

/// `{"done": {"ok": value}}` or `{"done": {"err": message}}`.
#[must_use]
pub fn done(result: Result<Value, String>) -> Value {
    match result {
        Ok(v) => serde_json::json!({ "done": { "ok": v } }),
        Err(e) => serde_json::json!({ "done": { "err": e } }),
    }
}
