//! `daw`: the engine as an Architect extension.
//!
//! It holds the open session (a `fts.session` document, kept in a
//! snapshot file), plays it with the `daw` engine ([`engine`]), and
//! provides two services through the host:
//!
//! - `fts.session` ([`session::extension::answer`]): the session
//!   primitive's own service, for every extension that reads or writes
//!   the session (actions `DAW_READ/WRITE/STRUCTURE`);
//! - `fts.daw` ([`daw_view`]): what the views of the DAW read and ask
//!   (actions `DAW_STATE`, `DAW_TRACKS`, `DAW_CLIPS`, `DAW_PEAKS`,
//!   `DAW_TRANSPORT`, `DAW_SET`, `DAW_OPEN`).
//!
//! Realtime state never goes through either: meters are written to the
//! engine's shared value table, which the host reads at compose; a fader
//! being dragged arrives the other way through the same table; the
//! playhead is the app's playhead deck, which this extension's source
//! drives ([`deck`]).

pub mod deck;
#[cfg(feature = "audio")]
pub mod engine;
pub mod import;
pub mod peaks;
pub mod rpp;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock, PoisonError};

use architect_extensions::linked::{Builder, Extension, Handle};
use daw_view::{Clip, Field, SetTrack, State, Track, Transport, fader};
use serde_json::{Value as Json, json};
use session::doc::SessionDoc;
use session::service::Sessions;
use session::{Entry, Session, Value};

/// The manifest.
pub const MANIFEST: &str = include_str!("../extension.toml");

/// The extension.
pub struct Daw;

static HANDLE: Mutex<Option<Handle>> = Mutex::new(None);

/// The runtime the process serves the host on: where calls to the host
/// (the playhead deck) run from the engine's threads.
pub static RUNTIME: OnceLock<tokio::runtime::Handle> = OnceLock::new();

/// The host, once the extension is built.
#[must_use]
pub fn handle() -> Option<Handle> {
    HANDLE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// The open session.
pub struct Open {
    /// Its document id (the file's stem until Architect hosts sessions).
    pub id: String,
    pub path: PathBuf,
    pub doc: SessionDoc,
}

#[derive(Default)]
struct Store {
    sessions: Sessions,
    open: Option<Open>,
    /// Bumped by this extension's own edits and opens; the document's
    /// own version covers everyone else's.
    bumps: u64,
}

static STORE: LazyLock<Mutex<Store>> = LazyLock::new(Mutex::default);

fn store() -> std::sync::MutexGuard<'static, Store> {
    STORE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A session's document from its snapshot file.
///
/// # Errors
///
/// It can't be read, or is not a Loro snapshot.
pub fn load(path: &Path) -> Result<SessionDoc, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let doc = loro::LoroDoc::new();
    doc.import(&bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(SessionDoc::from_doc(doc))
}

/// Writes `doc` to `path` as a snapshot.
///
/// # Errors
///
/// It can't be written.
pub fn save(doc: &SessionDoc, path: &Path) -> Result<(), String> {
    doc.doc().commit();
    let bytes = doc
        .doc()
        .export(loro::ExportMode::Snapshot)
        .map_err(|e| e.to_string())?;
    let tmp = path.with_extension("session.new");
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// A new session document holding `s`.
///
/// # Errors
///
/// The document refused it.
pub fn doc_of(s: &Session) -> Result<SessionDoc, String> {
    let doc = SessionDoc::new();
    doc.write(s).map_err(|e| e.to_string())?;
    doc.doc().commit();
    Ok(doc)
}

/// Opens the session at `path`, serves it, and plays it.
///
/// # Errors
///
/// It could not be read.
pub fn open(path: &Path) -> Result<String, String> {
    let doc = load(path)?;
    let id = path.file_stem().map_or_else(
        || "session".to_owned(),
        |s| s.to_string_lossy().into_owned(),
    );
    let session = doc.read();
    {
        let mut s = store();
        if let Some(old) = s.open.take() {
            s.sessions.close(&old.id);
        }
        s.sessions
            .open(id.clone(), SessionDoc::from_doc(doc.doc().clone()));
        s.open = Some(Open {
            id: id.clone(),
            path: path.to_owned(),
            doc,
        });
        s.bumps += 1;
    }
    #[cfg(feature = "audio")]
    engine::load(&session);
    deck::publish(&session);
    peaks::start(&session);
    Ok(id)
}

/// Something the views read moved (peaks computed).
pub fn bump() {
    store().bumps += 1;
}

/// The open session, read whole.
fn read_open() -> Option<(String, Session)> {
    let s = store();
    let open = s.open.as_ref()?;
    Some((open.id.clone(), open.doc.read()))
}

/// How far the open session has moved: every operation in its document,
/// whoever made it, plus this extension's opens.
fn revision() -> u64 {
    let s = store();
    let ops: u64 = s.open.as_ref().map_or(0, |o| {
        o.doc
            .doc()
            .oplog_vv()
            .iter()
            .map(|(_, c)| u64::try_from(*c).unwrap_or(0))
            .sum()
    });
    ops + s.bumps
}

fn state() -> State {
    let open = read_open();
    #[cfg(feature = "audio")]
    let (playing, position, duration, sample_rate, problem, values) = engine::status();
    #[cfg(not(feature = "audio"))]
    let (playing, position, duration, sample_rate, problem, values) = (
        false,
        0.0,
        0.0,
        0,
        Some("built without audio".to_owned()),
        None::<String>,
    );
    State {
        session: open.as_ref().map(|(id, _)| id.clone()),
        name: open
            .as_ref()
            .map(|(_, s)| s.name.clone())
            .unwrap_or_default(),
        revision: revision(),
        playing,
        position,
        duration,
        sample_rate,
        values,
        problem,
    }
}

fn map_value<'a>(m: &'a BTreeMap<String, Value>, key: &str) -> Option<&'a Value> {
    m.get(key)
}

#[expect(clippy::cast_precision_loss, reason = "small numbers")]
fn num(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Float(f) => Some(*f),
        Value::Int(i) => Some(*i as f64),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        _ => None,
    }
}

/// The tracks of the open session, in the engine's order.
#[must_use]
pub fn tracks() -> Vec<Track> {
    let Some((_, s)) = read_open() else {
        return Vec::new();
    };
    let (tracks, _) = import::engine_tracks(&s);
    let data = s.ext(&import::owner());
    tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let m = match data.and_then(|d| d.get(&format!("track:{}", t.lane))) {
                Some(Entry::Map(m)) => m.clone(),
                _ => BTreeMap::new(),
            };
            Track {
                lane: t.lane.to_string(),
                index: u32::try_from(i).unwrap_or(u32::MAX),
                name: t.track.name.clone(),
                color: t.track.color,
                depth: t.track.depth,
                volume: num(map_value(&m, "volume")).unwrap_or(fader::UNITY),
                pan: num(map_value(&m, "pan")).unwrap_or(0.0),
                mute: num(map_value(&m, "mute")).is_some_and(|v| v != 0.0),
                solo: num(map_value(&m, "solo")).is_some_and(|v| v != 0.0),
            }
        })
        .collect()
}

/// The clips of the open session.
#[must_use]
pub fn clips() -> Vec<Clip> {
    let Some((_, s)) = read_open() else {
        return Vec::new();
    };
    let (tracks, _) = import::engine_tracks(&s);
    let mut out = Vec::new();
    for t in &tracks {
        for (clip, (id, source)) in t.track.items.iter().zip(&t.clips) {
            out.push(Clip {
                id: id.to_string(),
                lane: t.lane.to_string(),
                start: clip.position,
                end: clip.position + clip.length,
                name: clip.name.clone(),
                color: clip.color,
                mute: clip.mute,
                gain: clip.gain,
                offset: clip.offset,
                peaks: source.as_deref().and_then(peaks::hash_of),
            });
        }
    }
    out
}

/// Sets a track's field in the document (one step), then in the engine.
///
/// # Errors
///
/// No session, no such track, or the document refused it.
pub fn set(req: &SetTrack) -> Result<(), String> {
    let index = tracks()
        .iter()
        .find(|t| t.lane == req.lane)
        .map(|t| t.index)
        .ok_or_else(|| format!("no track {}", req.lane))?;
    {
        let mut s = store();
        let open = s.open.as_ref().ok_or("no session is open")?;
        let owner = import::owner();
        let w = open.doc.ext(&owner).map_err(|e| e.to_string())?;
        let map = w
            .map(&format!("track:{}", req.lane))
            .map_err(|e| e.to_string())?;
        let (key, value): (&str, loro::LoroValue) = match req.field {
            Field::Volume => ("volume", req.value.clamp(0.0, 1.0).into()),
            Field::Pan => ("pan", req.value.clamp(-1.0, 1.0).into()),
            Field::Mute => ("mute", (req.value != 0.0).into()),
            Field::Solo => ("solo", (req.value != 0.0).into()),
        };
        map.insert(key, value).map_err(|e| e.to_string())?;
        let path = open.path.clone();
        save(&open.doc, &path)?;
        s.bumps += 1;
    }
    #[cfg(feature = "audio")]
    engine::set(index, req.field, req.value);
    let _ = index;
    Ok(())
}

fn parse<T: serde::de::DeserializeOwned>(args: &Json) -> Result<T, String> {
    serde_json::from_value(args.clone()).map_err(|e| format!("arguments: {e}"))
}

fn to_json<T: serde::Serialize>(v: &T) -> Result<Json, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

/// `fts.session`, answered for the sessions open here.
fn session_call(method: &str, args: &Json, cx: &Json) -> Result<Json, String> {
    let sessions = store().sessions.clone();
    let caller = cx.get("caller").and_then(Json::as_str);
    let reply = session::extension::answer(&sessions, method, &args.to_string(), caller)?;
    if method != session::service::READ {
        // Someone else's edit: it reaches the views by the revision.
        let s = store();
        if let Some(open) = &s.open {
            let _ = save(&open.doc, &open.path);
        }
    }
    serde_json::from_str(&reply).map_err(|e| e.to_string())
}

impl Extension for Daw {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        *HANDLE.lock().unwrap_or_else(PoisonError::into_inner) = Some(ext.handle());
        ext.action_in("DAW_READ", |args, cx| async move {
            session_call(session::service::READ, &args, &cx)
        })
        .action_in("DAW_WRITE", |args, cx| async move {
            session_call(session::service::WRITE, &args, &cx)
        })
        .action_in("DAW_STRUCTURE", |args, cx| async move {
            session_call(session::service::STRUCTURE, &args, &cx)
        })
        .action("DAW_STATE", |_| async { to_json(&state()) })
        .action("DAW_TRACKS", |_| async { to_json(&tracks()) })
        .action("DAW_CLIPS", |_| async { to_json(&clips()) })
        .action("DAW_PEAKS", |args| async move {
            let hash = args.get("hash").and_then(Json::as_str).ok_or("no hash")?;
            peaks::get(hash).map_or_else(|| Err(format!("no peaks {hash} (yet)")), |p| to_json(&p))
        })
        .action("DAW_TRANSPORT", |args| async move {
            let t: Transport = parse(&args)?;
            #[cfg(feature = "audio")]
            engine::transport(t);
            let _ = t;
            to_json(&state())
        })
        .action("DAW_SET", |args| async move {
            let req: SetTrack = parse(&args)?;
            set(&req)?;
            Ok(json!({ "ok": true }))
        })
        .action("DAW_OPEN", |args| async move {
            let path = args.get("path").and_then(Json::as_str).ok_or("no path")?;
            let id = open(Path::new(path))?;
            Ok(json!({ "session": id }))
        })
        .action("DAW_DECK", |args| async move {
            // The app's playhead moved by someone else (its bar's buttons):
            // the engine follows.
            deck::heard(&args);
            Ok(json!({ "ok": true }))
        });
    }
}
