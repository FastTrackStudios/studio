//! `studio.arrangement`: Studio's arrangement view, the one `studio-desktop`
//! shows, running in an Architect pane as a process extension.
//!
//! Nothing is redrawn or re-modelled here. The process is a Studio window
//! on Studio's engine (`studio_view_link`: attached as `studio-desktop`'s
//! window attaches to a `studio engine`, the song read through the window
//! protocol's replicas), and mounts the arrangement panel
//! (`session_ui::panel::use_arrangement_panel`, its widget and its DOM
//! chrome, `PanelChrome`) with an Architect pane as the panel's host
//! ([`host`]): the panel's third host, beside dioxus-native and the
//! browser.
//!
//! Its actions are the arrangement window's controls Architect has no
//! chrome for: the transport (`STUDIO_ARRANGEMENT_PLAY_PAUSE`, `_STOP`)
//! and the Folded switch (`STUDIO_ARRANGEMENT_FOLDED`), on keys, ⌘K and
//! MCP as every action is.

pub mod host;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use architect_extensions::linked::{Builder, Extension, PaintPane};
use architect_workspace::paint::PaintMount;
use dioxus::prelude::*;
use serde_json::{Value as Json, json};
use session_ui::song_model::StudioSession;
pub use studio_view_link::attach;
use studio_view_link::{InScope, Stats};

use crate::host::Hosted;

pub const MANIFEST: &str = include_str!("../extension.toml");

/// The latest pane's frame statistics, for `STUDIO_ARRANGEMENT_STATE`.
static STATS: Mutex<Option<Arc<Mutex<Stats>>>> = Mutex::new(None);

/// The track panel's shape: compact (the track's name) or expanded (its
/// controls), as the pane last drew it ([`host::Hosted`] keeps it), and a
/// shape `STUDIO_ARRANGEMENT_PANEL` asked for, which the pane takes up on
/// its next frame (the window's toolbar switch, or a swipe).
pub(crate) static COMPACT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
pub(crate) static ASKED_COMPACT: Mutex<Option<bool>> = Mutex::new(None);

pub struct Arrangement;

impl Extension for Arrangement {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        ext.view("arrangement", ArrangementPane)
            .action("STUDIO_ARRANGEMENT_STATE", |_| async { Ok(state()) })
            .action("STUDIO_ARRANGEMENT_PLAY_PAUSE", |_| async {
                studio_view_link::run("SESSION_PLAY_PAUSE", &Json::Null)
            })
            .action("STUDIO_ARRANGEMENT_STOP", |_| async {
                studio_view_link::run("SESSION_STOP", &Json::Null)
            })
            .action("STUDIO_ARRANGEMENT_FOLDED", |args| async move {
                let on = args
                    .get("on")
                    .and_then(Json::as_bool)
                    .unwrap_or_else(|| !session_ui::folds::tops());
                session_ui::folds::set_tops(on);
                Ok(json!({ "folded": on }))
            })
            .action("STUDIO_ARRANGEMENT_PANEL", |args| async move {
                let compact = args
                    .get("compact")
                    .and_then(Json::as_bool)
                    .unwrap_or_else(|| !COMPACT.load(std::sync::atomic::Ordering::Acquire));
                *ASKED_COMPACT.lock().unwrap_or_else(PoisonError::into_inner) = Some(compact);
                Ok(json!({ "compact": compact }))
            });
    }
}

fn state() -> Json {
    let song = studio_view_link::current().map(|(key, s)| (key, s.project.tracks.len()));
    let stats = STATS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .map(|s| s.lock().unwrap_or_else(PoisonError::into_inner).json());
    json!({
        "linked": session_engine::view_client::is_linked(),
        "problem": studio_view_link::problem(),
        "song": song.as_ref().map(|(k, _)| k.clone()),
        "tracks": song.as_ref().map(|(_, n)| *n),
        "folded": session_ui::folds::tops(),
        "compact": COMPACT.load(std::sync::atomic::Ordering::Acquire),
        "frames": stats,
    })
}

/// The pane: the arrangement once the engine has a song, a line saying
/// what it waits for until then.
#[component]
fn ArrangementPane() -> Element {
    let song = studio_view_link::use_song();
    let surface = studio_view_link::surface();
    match song() {
        Some((key, session)) => rsx! {
            div { style: "position:relative; width:100%; height:100%; background:{surface};",
                Panel { key: "{key}", session: Song(session) }
            }
        },
        None => studio_view_link::waiting(),
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
    use_context_provider(|| Signal::new(studio_view_link::mode_from_env()));
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
        let scope = InScope::here();
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
        {studio_view_link::studio_style()}
        div {
            class: studio_view_link::ROOT_CLASS,
            style: "position:absolute; top:0; left:0; right:0; bottom:0; overflow:hidden;",
            onmounted: move |event| {
                *mounted.borrow_mut() = Some(event.data());
            },
            div { style: "position:absolute; left:0; top:0; width:100%; height:100%;",
                PaintPane { mount }
            }
            session_ui::panel::PanelChrome { panel }
        }
    }
}
