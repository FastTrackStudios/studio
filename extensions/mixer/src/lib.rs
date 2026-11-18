//! `studio.mixer`: Studio's mixer and its track control panel as a
//! runtime extension.
//!
//! Two views of the engine's tracks (`fts.daw`): `mixer`, a strip per
//! track, and `tcp`, a row per track for a pane beside the arrangement.
//! Faders and pans are the host's drags, meters the engine's values
//! ([`view`]); a release or a press writes the document through `fts.daw
//! set`. It names no other view: the arrangement may be installed or not.

pub mod view;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use architect_extensions::hostcall::HostError;
use architect_extensions::linked::{Builder, Extension, Handle, PaintPane};
use architect_workspace::HostData;
use architect_workspace::paint::{FrameStats, PaintMount, PaintWake};
use daw_view::{SERVICE as DAW, State, Track, method};
use dioxus::prelude::*;
use futures_channel::mpsc::unbounded;
use futures_util::StreamExt as _;
use serde_json::{Value as Json, json};

use crate::view::{MixerView, Model, Set, Shape, Shared};

pub const MANIFEST: &str = include_str!("../extension.toml");

/// How often the view asks whether the engine's data moved.
pub const POLL: std::time::Duration = std::time::Duration::from_millis(250);

static HANDLE: Mutex<Option<Handle>> = Mutex::new(None);

fn handle() -> Option<Handle> {
    HANDLE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

#[derive(Clone)]
struct Open {
    shape: Shape,
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

pub struct Mixer;

impl Extension for Mixer {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        *HANDLE.lock().unwrap_or_else(PoisonError::into_inner) = Some(ext.handle());
        ext.view("mixer", MixerPane)
            .view("tcp", TcpPane)
            .action("STUDIO_MIXER_STATE", |_| async { Ok(state()) })
            .action("STUDIO_MIXER_FIXTURE", |args| async move { fixture(&args) });
    }
}

async fn daw<T: serde::de::DeserializeOwned>(method: &str, args: Json) -> Result<T, String> {
    let h = handle().ok_or("not running")?;
    let v = h.service(DAW, method, args).await.map_err(|e| match e {
        HostError::Failed { message } => message,
        other => format!("{other:?}"),
    })?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// Keeps `model` current with the engine's tracks.
async fn follow(model: HostData<Model>, wake: PaintWake) {
    let mut seen = None;
    loop {
        if model.read(|m| m.revision) < FIXTURE_REVISION {
            match daw::<State>(method::STATE, json!({})).await {
                Ok(s) if seen != Some(s.revision) => {
                    if let Ok(tracks) = daw::<Vec<Track>>(method::TRACKS, json!({})).await {
                        model.update(|m| {
                            m.tracks = tracks;
                            m.engine = true;
                            m.problem = s.problem.clone();
                            m.revision += 1;
                        });
                        seen = Some(s.revision);
                        wake.wake();
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    if model.read(|m| m.problem.as_deref() != Some(e.as_str())) {
                        model.update(|m| {
                            m.engine = false;
                            m.problem = Some(e);
                            m.revision += 1;
                        });
                        wake.wake();
                    }
                }
            }
        }
        futures_timer::Delay::new(POLL).await;
    }
}

/// A fixture's revisions start here, so the poll leaves it alone.
const FIXTURE_REVISION: u64 = 1 << 40;

fn pane(shape: Shape) -> Element {
    let key = use_hook(|| NEXT.fetch_add(1, Ordering::Relaxed));
    let (mount, open, rx) = use_hook(|| {
        let model = HostData::new(Model::default());
        let shared = HostData::new(Shared::default());
        let (tx, rx) = unbounded::<Set>();
        let mount = {
            let (model, shared) = (model.clone(), shared.clone());
            PaintMount::new(move |_| {
                let tx = tx.clone();
                MixerView::new(
                    shape,
                    model.clone(),
                    shared.clone(),
                    Box::new(move |s| {
                        let _ = tx.unbounded_send(s);
                    }),
                )
            })
        };
        let open = Open {
            shape,
            model,
            shared,
            stats: mount.frame_stats(),
            wake: mount.wake_handle(),
        };
        open_panes().insert(key, open.clone());
        (mount, open, Arc::new(Mutex::new(Some(rx))))
    });
    use_hook(|| {
        spawn(follow(open.model.clone(), open.wake.clone()));
        if let Some(mut rx) = rx.lock().unwrap_or_else(PoisonError::into_inner).take() {
            spawn(async move {
                while let Some(set) = rx.next().await {
                    let _ = match set {
                        Set::Track { lane, field, value } => {
                            let field = serde_json::to_value(field).unwrap_or(Json::Null);
                            daw::<Json>(
                                method::SET,
                                json!({ "lane": lane, "field": field, "value": value }),
                            )
                            .await
                        }
                        Set::Transport(t) => {
                            let t = serde_json::to_value(t).unwrap_or(Json::Null);
                            daw::<Json>(method::TRANSPORT, t).await
                        }
                    };
                }
            });
        }
    });
    use_drop(move || {
        open_panes().remove(&key);
    });
    rsx! {
        div { style: "position:relative; width:100%; height:100%; min-height:0;",
            PaintPane { mount: mount.clone() }
        }
    }
}

#[component]
fn MixerPane() -> Element {
    pane(Shape::Strips)
}

#[component]
fn TcpPane() -> Element {
    pane(Shape::Rows)
}

fn ms(d: std::time::Duration) -> f64 {
    (d.as_secs_f64() * 100_000.0).round() / 100.0
}

fn state() -> Json {
    let panes: Vec<Json> = open_panes()
        .values()
        .map(|o| {
            let f = o.stats.summary();
            let (tracks, engine, problem) = o.model.read(|m| (m.tracks.len(), m.engine, m.problem.clone()));
            o.shared.read(|s| {
                json!({
                    "view": if o.shape == Shape::Strips { "mixer" } else { "tcp" },
                    "tracks": tracks,
                    "engine": engine,
                    "problem": problem,
                    "scroll": s.scroll,
                    "layers": s.layers,
                    "records": s.records,
                    "region_events": s.heard,
                    "drag_values_heard": s.values_heard,
                    "sets": s.sets,
                    "frames": {"count": f.frames, "p50_ms": ms(f.p50), "p99_ms": ms(f.p99), "max_ms": ms(f.max)},
                })
            })
        })
        .collect();
    json!({ "panes": panes })
}

/// `{tracks}` synthetic tracks (a measurement's size), or the engine's
/// again (`{}`).
fn fixture(args: &Json) -> Result<Json, String> {
    let n = args
        .get("tracks")
        .and_then(Json::as_u64)
        .and_then(|v| u32::try_from(v).ok());
    for o in open_panes().values() {
        match n {
            Some(n) => o.model.update(|m| {
                m.tracks = (0..n.min(daw_view::names::MAX_TRACKS))
                    .map(|i| Track {
                        lane: format!("fixture-{i}"),
                        index: i,
                        name: format!("Track {}", i + 1),
                        color: Some(0x40_80_c0 ^ (i * 0x10_20_30 & 0xff_ff_ff)),
                        volume: daw_view::fader::UNITY,
                        ..Track::default()
                    })
                    .collect();
                m.revision = FIXTURE_REVISION + u64::from(n);
            }),
            None => o.model.update(|m| m.revision = 0),
        }
        o.wake.wake();
    }
    Ok(state())
}
