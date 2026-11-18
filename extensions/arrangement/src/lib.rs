//! `studio.arrangement`: Studio's arrangement view as a runtime extension.
//!
//! It reads the session through `fts.session` (every lane and item,
//! whoever owns them: an Ignition cue lane shows as well as a track) and,
//! when an engine provides it, the DAW's own data through `fts.daw`
//! (names, colours, mixer state, waveform peaks by hash). It names no
//! other view: the mixer may be installed or not, and anything that
//! provides the two services can stand in for the engine.
//!
//! The view itself is [`view::ArrangeView`], a paint view of layers the
//! host keeps; this file is the pane around it and the reads that feed it.

pub mod model;
pub mod view;

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use architect_extensions::hostcall::HostError;
use architect_extensions::linked::{Builder, Extension, Handle, PaintPane, use_pane_state};
use architect_workspace::HostData;
use architect_workspace::paint::{FrameStats, PaintMount, PaintWake};
use daw_view::{SERVICE as DAW, State, method};
use dioxus::prelude::*;
use futures_channel::mpsc::{UnboundedSender, unbounded};
use futures_util::StreamExt as _;
use serde_json::{Value as Json, json};
use session::service::Request;

use crate::model::{Model, Wave};
use crate::view::{ArrangeView, Camera, Intent, Shared};

pub const MANIFEST: &str = include_str!("../extension.toml");
pub const ID: &str = "studio.arrangement";
const SESSION: &str = "fts.session";

/// How often the view asks whether anything moved.
pub const POLL: std::time::Duration = std::time::Duration::from_millis(250);

static HANDLE: Mutex<Option<Handle>> = Mutex::new(None);

fn handle() -> Option<Handle> {
    HANDLE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// A pane showing the arrangement.
#[derive(Clone)]
struct Open {
    model: HostData<Model>,
    shared: HostData<Shared>,
    stats: FrameStats,
    wake: PaintWake,
}

static OPEN: Mutex<BTreeMap<u64, Open>> = Mutex::new(BTreeMap::new());
static NEXT: AtomicU64 = AtomicU64::new(0);

fn open_panes() -> std::sync::MutexGuard<'static, BTreeMap<u64, Open>> {
    OPEN.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct Arrangement;

impl Extension for Arrangement {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        *HANDLE.lock().unwrap_or_else(PoisonError::into_inner) = Some(ext.handle());
        ext.view("arrangement", ArrangementPane)
            .action("ARRANGEMENT_STATE", |_| async { Ok(state()) })
            .action("ARRANGEMENT_VIEW", |args| async move { set_view(&args) })
            .action("ARRANGEMENT_FIXTURE", |args| async move { fixture(&args) });
    }
}

async fn call(service: &str, method: &str, args: Json) -> Result<Json, String> {
    let h = handle().ok_or("not running")?;
    h.service(service, method, args).await.map_err(|e| match e {
        HostError::Failed { message } => message,
        other => format!("{other:?}"),
    })
}

async fn daw<T: serde::de::DeserializeOwned>(method: &str, args: Json) -> Result<T, String> {
    let v = call(DAW, method, args).await?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}

async fn session_read(doc: &str, request: Request) -> Result<session::service::Response, String> {
    let args = session::extension::call_json(doc, request)?;
    let args: Json = serde_json::from_str(&args).map_err(|e| e.to_string())?;
    let v = call(SESSION, "read", args).await?;
    session::extension::response_of(&v.to_string())
}

/// Reads everything again: the session's timeline and structure, the
/// engine's tracks and clips, and peaks for hashes not held yet.
async fn read_all(doc: &str, engine: bool, held: &HashSet<String>) -> Result<Model, String> {
    let timeline = session_read(doc, Request::ReadTimeline).await?;
    let structure = session_read(doc, Request::ReadStructure).await?;
    let mut m = model::generic(&timeline, &structure);
    if engine {
        let tracks: Vec<daw_view::Track> = daw(method::TRACKS, json!({})).await?;
        let clips: Vec<daw_view::Clip> = daw(method::CLIPS, json!({})).await?;
        let wanted: HashSet<String> = clips
            .iter()
            .filter_map(|c| c.peaks.clone())
            .filter(|h| !held.contains(h))
            .collect();
        for hash in wanted {
            if let Ok(p) = daw::<daw_view::Peaks>(method::PEAKS, json!({ "hash": hash })).await
                && let Ok(bytes) = daw_view::peaks::decode(&p.data)
            {
                m.waves.insert(
                    hash,
                    Arc::new(Wave {
                        per_second: f64::from(p.sample_rate) / f64::from(daw_view::PEAK_FRAMES),
                        pairs: daw_view::peaks::pairs(&bytes),
                    }),
                );
            }
        }
        model::with_daw(&mut m, &tracks, &clips);
    }
    Ok(m)
}

/// Keeps `model` current: asks the engine's revision every [`POLL`] and
/// reads again when it moved; without an engine, reads the session
/// `pane_doc` names every few polls.
async fn follow(model: HostData<Model>, wake: PaintWake, pane_doc: String) {
    let mut seen: Option<u64> = None;
    let mut polls = 0_u64;
    loop {
        if !model.read(|m| m.fixture) {
            let state: Result<State, String> = daw(method::STATE, json!({})).await;
            let (engine, doc, revision, problem) = match &state {
                Ok(s) => (true, s.session.clone(), Some(s.revision), s.problem.clone()),
                Err(e) => (
                    false,
                    Some(pane_doc.clone()).filter(|d| !d.is_empty()),
                    None,
                    Some(e.clone()),
                ),
            };
            let due = match revision {
                Some(r) => seen != Some(r),
                None => polls % 8 == 0,
            };
            if let Some(doc) = doc.filter(|_| due) {
                let held: HashSet<String> = model.read(|m| m.waves.keys().cloned().collect());
                match read_all(&doc, engine, &held).await {
                    Ok(mut fresh) => {
                        model.update(|m| {
                            // Waves are kept by hash: what was fetched stays.
                            for (h, w) in m.waves.drain() {
                                fresh.waves.entry(h).or_insert(w);
                            }
                            fresh.revision = m.revision + 1;
                            fresh.session = Some(doc.clone());
                            fresh.name = if fresh.name.is_empty() {
                                state.as_ref().map(|s| s.name.clone()).unwrap_or_default()
                            } else {
                                fresh.name.clone()
                            };
                            fresh.problem = problem.clone();
                            *m = fresh;
                        });
                        seen = revision;
                        wake.wake();
                    }
                    Err(e) => model.update(|m| m.problem = Some(e)),
                }
            } else if problem != model.read(|m| m.problem.clone()) {
                model.update(|m| m.problem = problem);
            }
        }
        polls += 1;
        futures_timer::Delay::new(POLL).await;
    }
}

/// What a press in the view asks of the engine.
async fn ask(intent: Intent) {
    let t = match intent {
        Intent::Seek(s) => json!({ "do": "seek", "seconds": s }),
        Intent::Toggle => json!({ "do": "toggle" }),
        Intent::Stop => json!({ "do": "stop" }),
    };
    let _ = call(DAW, method::TRANSPORT, t).await;
}

#[component]
fn ArrangementPane() -> Element {
    // A pane's state may name a session to read without an engine.
    let pane_doc = use_pane_state().map(|(_, s)| s).unwrap_or_default();
    let key = use_hook(|| NEXT.fetch_add(1, Ordering::Relaxed));
    let (mount, open, tx) = use_hook(|| {
        let model = HostData::new(Model::default());
        let shared = HostData::new(Shared::default());
        let (tx, rx) = unbounded::<Intent>();
        let mount = {
            let (model, shared, tx) = (model.clone(), shared.clone(), tx.clone());
            PaintMount::new(move |_| {
                let tx: UnboundedSender<Intent> = tx.clone();
                ArrangeView::new(
                    model.clone(),
                    shared.clone(),
                    Box::new(move |i| {
                        let _ = tx.unbounded_send(i);
                    }),
                )
            })
        };
        let open = Open {
            model,
            shared,
            stats: mount.frame_stats(),
            wake: mount.wake_handle(),
        };
        open_panes().insert(key, open.clone());
        (mount, open, Arc::new(Mutex::new(Some(rx))))
    });
    use_hook(|| {
        let (model, wake) = (open.model.clone(), open.wake.clone());
        spawn(follow(model, wake, pane_doc.clone()));
        if let Some(mut rx) = tx.lock().unwrap_or_else(PoisonError::into_inner).take() {
            spawn(async move {
                while let Some(intent) = rx.next().await {
                    ask(intent).await;
                }
            });
        }
    });
    use_drop(move || {
        open_panes().remove(&key);
    });
    rsx! {
        div { style: "display:flex; flex-direction:column; width:100%; height:100%; min-height:0;",
            div { style: "flex:1; min-height:0;",
                PaintPane { mount: mount.clone() }
            }
        }
    }
}

fn ms(d: std::time::Duration) -> f64 {
    (d.as_secs_f64() * 100_000.0).round() / 100.0
}

/// Every open arrangement: its camera, size, selection, and its frames.
fn state() -> Json {
    let panes: Vec<Json> = open_panes()
        .values()
        .map(|o| {
            let f = o.stats.summary();
            let (rows, clips, revision, engine, problem, waves, session) = o.model.read(|m| {
                (
                    m.rows.len(),
                    m.clip_count(),
                    m.revision,
                    m.engine,
                    m.problem.clone(),
                    m.waves.len(),
                    m.session.clone(),
                )
            });
            o.shared.read(|s| {
                json!({
                    "session": session,
                    "rows": rows,
                    "clips": clips,
                    "waves": waves,
                    "revision": revision,
                    "engine": engine,
                    "problem": problem,
                    "camera": s.camera.map(|c| json!({"left": c.left, "top": c.top, "zoom": c.zoom})),
                    "selection": s.selection,
                    "records": s.records,
                    "layers": s.layers,
                    "frames": {"count": f.frames, "p50_ms": ms(f.p50), "p99_ms": ms(f.p99), "max_ms": ms(f.max)},
                })
            })
        })
        .collect();
    json!({ "panes": panes })
}

/// Moves every open arrangement's camera (a script's pan and zoom).
fn set_view(args: &Json) -> Result<Json, String> {
    for o in open_panes().values() {
        let now = o.shared.read(|s| s.camera).unwrap_or_default();
        let get = |k: &str, d: f64| args.get(k).and_then(Json::as_f64).unwrap_or(d);
        let asked = Camera {
            left: get("left", now.left),
            top: get("top", now.top),
            zoom: get("zoom", now.zoom),
        };
        o.shared.update(|s| s.asked = Some(asked));
        o.wake.wake();
    }
    Ok(state())
}

/// Shows a synthetic arrangement (`{tracks, clips}`), or the session
/// again (`{}`).
fn fixture(args: &Json) -> Result<Json, String> {
    let n = |k: &str| {
        args.get(k)
            .and_then(Json::as_u64)
            .and_then(|v| u32::try_from(v).ok())
    };
    for o in open_panes().values() {
        match (n("tracks"), n("clips")) {
            (Some(t), Some(c)) => {
                let mut m = model::fixture(t, c);
                m.revision = o.model.read(|m| m.revision) + 1;
                o.model.set(m);
            }
            _ => o.model.update(|m| m.fixture = false),
        }
        o.wake.wake();
    }
    Ok(state())
}
