//! The surface this process shows, one for every view of it: the document
//! it was loaded from (a `.frame` file, watched: a save reloads it), where
//! its params and feeds are, and a revision each change bumps, which the
//! views follow ([`changes`]).
//!
//! `frame-surface`'s actions are these functions: [`load`], [`set_param`],
//! [`feed`], [`state`].

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

use frame::embed::Pointer;
use serde_json::{Value, json};
use tokio::sync::watch;

use crate::surface::{DOCUMENT, Live};
use crate::svg::Svg;

/// How wide the surface is laid out, px.
pub const WIDTH: f64 = 212.0;

/// How often a loaded document is checked for a save.
pub const POLL: Duration = Duration::from_millis(200);

struct Shown {
    live: Result<Live, String>,
    /// The file it was loaded from; none while it shows [`DOCUMENT`].
    path: Option<PathBuf>,
    /// The file's modification time when it was last read.
    modified: Option<SystemTime>,
    /// How many times a document has been read in: a load, a reload.
    loads: u64,
    /// Why the last read of the file did not take; the surface stays as
    /// it was.
    error: Option<String>,
}

static SHOWN: LazyLock<Mutex<Shown>> = LazyLock::new(|| {
    Mutex::new(Shown {
        live: Live::new(DOCUMENT, WIDTH),
        path: None,
        modified: None,
        loads: 0,
        error: None,
    })
});

static REVISION: LazyLock<watch::Sender<u64>> = LazyLock::new(|| watch::Sender::new(0));

fn shown() -> MutexGuard<'static, Shown> {
    SHOWN.lock().unwrap_or_else(PoisonError::into_inner)
}

fn bump() {
    REVISION.send_modify(|r| *r += 1);
}

/// The revision, bumped by each change: a view redraws when it moves.
#[must_use]
pub fn changes() -> watch::Receiver<u64> {
    REVISION.subscribe()
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn live(s: &mut Shown) -> Result<&mut Live, String> {
    s.live.as_mut().map_err(|e| e.clone())
}

/// Reads `s.path` in, keeping the surface as it was when it does not
/// parse.
fn read(s: &mut Shown) -> Result<(), String> {
    let Some(path) = s.path.clone() else {
        return Ok(());
    };
    s.modified = modified(&path);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let done = if let Ok(live) = s.live.as_mut() {
        live.reload(&text)
    } else {
        Live::new(&text, WIDTH).map(|live| s.live = Ok(live))
    };
    done.map_err(|e| format!("{} does not parse: {e}", path.display()))?;
    s.loads += 1;
    Ok(())
}

/// Shows the document at `path` and watches it ([`watch`]): its state.
///
/// # Errors
///
/// It cannot be read, or does not parse; the surface stays as it was.
pub fn load(path: &Path) -> Result<Value, String> {
    let mut s = shown();
    let was = (s.path.clone(), s.modified);
    s.path = Some(path.to_owned());
    match read(&mut s) {
        Ok(()) => {
            s.error = None;
            drop(s);
            bump();
            Ok(state())
        }
        Err(e) => {
            (s.path, s.modified) = was;
            Err(e)
        }
    }
}

/// Reads the loaded document in again if it was saved since: whether it
/// was. A save that does not parse leaves the surface as it was and says
/// why in [`state`]'s `error`.
pub fn check() -> bool {
    let mut s = shown();
    let Some(path) = s.path.clone() else {
        return false;
    };
    if modified(&path) == s.modified {
        return false;
    }
    s.error = read(&mut s).err();
    drop(s);
    bump();
    true
}

/// Checks the loaded document for a save every [`POLL`], on a thread of
/// its own; once a process.
pub fn watch() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        std::thread::spawn(|| {
            loop {
                std::thread::sleep(POLL);
                check();
            }
        });
    });
}

/// Moves the param `addr` (0..1): the state.
///
/// # Errors
///
/// The surface has no such param, or no surface loaded.
pub fn set_param(addr: &str, value: f64) -> Result<Value, String> {
    live(&mut shown())?.set_param(addr, value)?;
    bump();
    Ok(state())
}

/// A reading of the feed `id` (dB, a value per channel): the state.
///
/// # Errors
///
/// The surface has no such feed, the reading has no value, or no surface
/// loaded.
pub fn feed(id: &str, values: &[f64]) -> Result<Value, String> {
    live(&mut shown())?.feed(id, values)?;
    bump();
    Ok(state())
}

/// Pointer input on the surface (surface-local px), from a view.
pub fn pointer(event: Pointer) {
    let moved = shown()
        .live
        .as_mut()
        .map(|live| live.pointer(event))
        .unwrap_or_default();
    if !moved.is_empty() {
        bump();
    }
}

/// Where the param `addr` is (0..1).
#[must_use]
pub fn param(addr: &str) -> Option<f64> {
    shown().live.as_ref().ok()?.param(addr)
}

/// The surface painted now, or why there is none.
///
/// # Errors
///
/// The first document did not parse.
pub fn paint() -> Result<Svg, String> {
    live(&mut shown()).map(Live::svg)
}

/// What it shows: `document` (the file, or null for the built-in one),
/// `loads`, `error` (why the last save did not take, or null), `params`
/// and `feeds` (`Live::json`).
#[must_use]
pub fn state() -> Value {
    let s = shown();
    let mut out = match &s.live {
        Ok(live) => live.json(),
        Err(_) => json!({ "params": [], "feeds": [] }),
    };
    if let Value::Object(map) = &mut out {
        map.insert(
            "document".into(),
            s.path
                .as_ref()
                .map_or(Value::Null, |p| p.display().to_string().into()),
        );
        map.insert("loads".into(), s.loads.into());
        let error = s.error.clone().or_else(|| s.live.as_ref().err().cloned());
        map.insert("error".into(), error.map_or(Value::Null, Value::from));
    }
    out
}
