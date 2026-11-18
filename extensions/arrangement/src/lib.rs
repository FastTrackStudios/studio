//! `studio.arrangement`: Studio's arrangement view, the one `studio-desktop`
//! shows, running in an Architect pane as a process extension.
//!
//! Nothing is redrawn or re-modelled here. The process is a Studio window
//! on Studio's engine: it attaches to the engine the `daw` extension runs
//! exactly as `studio-desktop`'s window attaches to a `studio engine`
//! (`session_engine::studio_engine::link`, `session_engine::open::attach`),
//! reads the song through the window protocol's replicas
//! (`session_engine::view_client`), and mounts the arrangement panel
//! (`session_ui::panel::use_arrangement_panel`, its widget and its DOM
//! chrome, `PanelChrome`) with an Architect pane as the panel's host
//! ([`host`]): the panel's third host, beside dioxus-native and the
//! browser.

pub mod host;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use architect_extensions::linked::{Builder, Extension, PaintPane};
use architect_workspace::paint::PaintMount;
use dioxus::prelude::*;
use serde_json::{Value as Json, json};
use session_engine::studio_engine::link;
use session_ui::song_model::StudioSession;

use crate::host::{Hosted, InScope, Stats};

pub const MANIFEST: &str = include_str!("../extension.toml");

/// How long a pane waits for an engine before it says so (it keeps
/// trying after).
const WAIT: std::time::Duration = std::time::Duration::from_secs(5);

static ATTACHING: AtomicBool = AtomicBool::new(false);
static PROBLEM: Mutex<Option<String>> = Mutex::new(None);
/// The latest pane's frame statistics, for `STUDIO_ARRANGEMENT_STATE`.
static STATS: Mutex<Option<Arc<Mutex<Stats>>>> = Mutex::new(None);

fn problem(why: Option<String>) {
    *PROBLEM.lock().unwrap_or_else(PoisonError::into_inner) = why;
}

/// Attach this process, a Studio window, to the engine: what
/// `studio-desktop`'s window does with `FTS_ENGINE=attach`
/// (`session_desktop::native::attach_engine`). Once per process, on a
/// thread of its own; it keeps the link after (an engine that restarts is
/// attached to again).
pub fn attach() {
    if ATTACHING.swap(true, Ordering::AcqRel) {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("studio-attach".to_owned())
        .spawn(|| {
            loop {
                match link::choose(link::Mode::Attach, WAIT).and_then(|found| attach_to(&found)) {
                    Ok(()) => {
                        problem(None);
                        link::keep_linked(link::Mode::Attach, attach_to);
                        return;
                    }
                    Err(e) => problem(Some(format!("waiting for Studio's engine: {e}"))),
                }
            }
        });
}

fn attach_to(found: &link::Found) -> Result<(), String> {
    let target = session_engine::open::RemoteTarget::Engine {
        socket: found.socket.clone(),
    };
    session_engine::open::set_mode(session_engine::open::ModeState::remote(
        target.clone(),
        false,
    ));
    session_engine::open::attach(&target).map_err(|e| e.to_string())?;
    link::set_linked(found.clone());
    Ok(())
}

/// The song the engine has current, as the window lays it out, with what
/// names it (a new song or a new set mounts the panel again).
fn current() -> Option<(String, StudioSession)> {
    session_engine::view_client::peek_set(|view| {
        let set = view.setlist.as_ref()?;
        let song = set.songs.get(set.at)?;
        Some((
            format!("{}:{}", view.epoch, song.project),
            song.session.clone(),
        ))
    })
}

/// The window's mode as Studio's shell has it: `FTS_SESSION_MODE`, else
/// Live (`session_desktop::native::shell`).
fn mode_from_env() -> session::modes::Mode {
    std::env::var("FTS_SESSION_MODE")
        .ok()
        .and_then(|name| {
            session::modes::Mode::ALL
                .into_iter()
                .find(|m| m.display_name().eq_ignore_ascii_case(&name))
        })
        .unwrap_or(session::modes::Mode::Live)
}

pub struct Arrangement;

impl Extension for Arrangement {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        ext.view("arrangement", ArrangementPane)
            .action("STUDIO_ARRANGEMENT_STATE", |_| async { Ok(state()) })
            .action("STUDIO_ARRANGEMENT_PLAY_PAUSE", |_| async {
                run("SESSION_PLAY_PAUSE", &Json::Null)
            })
            .action("STUDIO_ARRANGEMENT_STOP", |_| async {
                run("SESSION_STOP", &Json::Null)
            })
            .action("STUDIO_ARRANGEMENT_FOLDED", |args| async move {
                let on = args
                    .get("on")
                    .and_then(Json::as_bool)
                    .unwrap_or(!session_ui::folds::tops());
                session_ui::folds::set_tops(on);
                Ok(json!({ "folded": on }))
            });
    }
}

/// An action of the engine's Action List, run over this window's link.
fn run(id: &str, args: &Json) -> Result<Json, String> {
    let out = session_engine::view_client::run(id, args.to_string().as_bytes())?;
    Ok(serde_json::from_str(&out).unwrap_or(Json::String(out)))
}

fn state() -> Json {
    let song = current().map(|(key, s)| (key, s.project.tracks.len()));
    let stats = STATS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .map(|s| s.lock().unwrap_or_else(PoisonError::into_inner).clone());
    let pct = |v: &std::collections::VecDeque<f64>, p: usize| {
        let mut s: Vec<f64> = v.iter().copied().collect();
        s.sort_by(f64::total_cmp);
        s.get((s.len().saturating_sub(1) * p) / 100)
            .copied()
            .unwrap_or(0.0)
    };
    json!({
        "linked": session_engine::view_client::is_linked(),
        "problem": PROBLEM.lock().unwrap_or_else(PoisonError::into_inner).clone(),
        "song": song.as_ref().map(|(k, _)| k.clone()),
        "tracks": song.as_ref().map(|(_, n)| *n),
        "folded": session_ui::folds::tops(),
        "frames": stats.as_ref().map(|s| json!({
            "count": s.frames,
            "changed": s.changed,
            "size": [s.size.0, s.size.1],
            "commands": s.commands,
            "work_p50_ms": pct(&s.work_ms, 50),
            "work_p99_ms": pct(&s.work_ms, 99),
        })),
    })
}

/// The pane: the arrangement once the engine has a song, a line saying
/// what it waits for until then.
#[component]
fn ArrangementPane() -> Element {
    attach();
    let mut song = use_signal(current);
    use_future(move || async move {
        loop {
            futures_timer::Delay::new(std::time::Duration::from_millis(200)).await;
            let now = current();
            if now.as_ref().map(|(k, _)| k) != song.peek().as_ref().map(|(k, _)| k) {
                song.set(now);
            }
        }
    });
    let colors = daw_ui::studio::lanes::Colors::from_theme(&daw_ui::theming::Theme::dark());
    let surface = colors.surface.clone();
    match song() {
        Some((key, session)) => rsx! {
            div { style: "position:relative; width:100%; height:100%; background:{surface};",
                Panel { key: "{key}", session: Song(session) }
            }
        },
        None => {
            let why = PROBLEM
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
                .unwrap_or_else(|| "Waiting for Studio's engine to open a song…".to_owned());
            rsx! {
                div { style: "padding:16px; font-size:13px; opacity:0.7; color:#ccc; background:{surface}; height:100%;",
                    "{why}"
                }
            }
        }
    }
}

/// A song's session as a prop (keyed by the pane, so never compared).
#[derive(Clone)]
struct Song(StudioSession);

impl PartialEq for Song {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// Studio's arrangement panel over `session`, hosted in the pane: what
/// `session_ui::studio::Arrangement` is in the desktop window, with the
/// pane in place of the window.
#[component]
fn Panel(session: Song) -> Element {
    use_context_provider(|| session.0.clone());
    use_context_provider(|| Signal::new(mode_from_env()));
    // Beside a mixer pane it is not linked to: compact, as docked.
    use_context_provider(|| session_ui::panel::StartCompact(true));
    let applier = use_hook(|| Rc::new(session_ui::engine::Applier::start()));
    let engine: session_ui::panel::Engine = {
        let applier = Rc::clone(&applier);
        Rc::new(move |edit| {
            if let Some(applier) = applier.as_ref() {
                applier.send(edit);
            }
        })
    };
    let (panel, widget) =
        session_ui::panel::use_arrangement_panel(None, engine, |w| Rc::new(RefCell::new(w)));
    let stats = use_hook(|| {
        let stats = Arc::new(Mutex::new(Stats::default()));
        *STATS.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&stats));
        stats
    });
    let mount = use_hook(|| {
        let scope = InScope {
            runtime: dioxus::core::Runtime::current(),
            scope: dioxus::core::current_scope_id(),
        };
        let (widget, panel, stats) = (Rc::clone(&widget), panel.clone(), Arc::clone(&stats));
        PaintMount::new(move |_| {
            Hosted::new(
                Rc::clone(&widget),
                panel.clone(),
                scope.clone(),
                Arc::clone(&stats),
            )
        })
    });
    let mounted = Rc::clone(&panel.mounted);
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; right:0; bottom:0; overflow:hidden;",
            onmounted: move |event| {
                *mounted.borrow_mut() = Some(event.data());
            },
            div { style: "position:absolute; left:0; top:0; width:100%; height:100%;",
                PaintPane { mount: mount.clone() }
            }
            session_ui::panel::PanelChrome { panel: panel.clone() }
        }
    }
}
