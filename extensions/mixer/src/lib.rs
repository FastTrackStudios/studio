//! `studio.mixer`: Studio's mixer and its selected-track strip, the ones
//! `studio-desktop` shows, in Architect panes as a process extension.
//!
//! Nothing is redrawn or re-modelled. The process is a Studio window on
//! Studio's engine (`studio_view_link`), and each pane hosts the mixer's
//! own widget (`session_ui::mixer_panel::HostedMixer`, the widget the
//! window's `Mixer` and the browser's `WebMixer` mount) over the song's
//! strips as the window plans them (`Planner::plan_strips`):
//!
//! - `mixer`: every strip, sending its own edits to the engine, as the
//!   window's mixer does with no arrangement beside it (`Links::alone`);
//! - `strip`: the selected track's strip and the folder it goes out
//!   through, as the window's inspector shows them (`Links::inspector`).
//!
//! `STUDIO_MIXER_RACKS` is the window's Racks switch. The window actions
//! the mixer carries out in `studio-desktop` (`MIXER_VIEW`, the racks
//! switch, the mixer's toggle) are this process's: it claims them from
//! the engine ([`claim`]).

//!
//! **In a browser** (studio-web) the `mixer` view is the same widget over
//! the page's own engine (`session_ui::web_host`, [`web`]); the strip and
//! the close-up say they are the desktop's for now.

#[cfg(not(target_arch = "wasm32"))]
pub mod claim;
#[cfg(not(target_arch = "wasm32"))]
pub mod closeup;
#[cfg(not(target_arch = "wasm32"))]
pub mod host;
#[cfg(target_arch = "wasm32")]
pub mod web;

#[cfg(not(target_arch = "wasm32"))]
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

#[cfg(not(target_arch = "wasm32"))]
use architect_extensions::linked::PaintPane;
use architect_extensions::linked::{Builder, Extension};
#[cfg(not(target_arch = "wasm32"))]
use architect_workspace::paint::PaintMount;
#[cfg(not(target_arch = "wasm32"))]
use dioxus::prelude::*;
use serde_json::{Value as Json, json};
#[cfg(not(target_arch = "wasm32"))]
use session_ui::mixer_panel::{HostedMixer, Links};
#[cfg(not(target_arch = "wasm32"))]
use session_ui::song_model::StudioSession;
#[cfg(not(target_arch = "wasm32"))]
use studio_view_link::InScope;
use studio_view_link::Stats;

#[cfg(not(target_arch = "wasm32"))]
use crate::host::Hosted;

pub const MANIFEST: &str = include_str!("../extension.toml");

/// One pane's frame statistics, and which view it is.
type PaneStats = (&'static str, Arc<Mutex<Stats>>);

/// Each pane's frame statistics, by view, for `STUDIO_MIXER_STATE`.
static STATS: Mutex<Vec<PaneStats>> = Mutex::new(Vec::new());

pub struct Mixer;

impl Extension for Mixer {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            claim::claim(ext.handle());
            ext.view("mixer", MixerPane)
                .view("strip", StripPane)
                .view("closeup", closeup::CloseupPane);
        }
        #[cfg(target_arch = "wasm32")]
        ext.view("mixer", web::MixerPane)
            .view("strip", web::StripPane)
            .view("closeup", web::CloseupPane);
        ext.action("STUDIO_MIXER_STATE", |_| async { Ok(state()) })
            .action("STUDIO_MIXER_RACKS", |args| async move {
                let on = args
                    .get("on")
                    .and_then(Json::as_bool)
                    .unwrap_or_else(|| !session_ui::mixer_panel::racks_shown());
                session_ui::mixer_panel::set_racks_shown(on);
                Ok(json!({ "racks": on }))
            });
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// A new pane's frame statistics, kept for `STUDIO_MIXER_STATE` as
/// `view`'s (the last pane of each view).
pub(crate) fn pane_stats(view: &'static str) -> Arc<Mutex<Stats>> {
    let stats = Arc::new(Mutex::new(Stats::default()));
    let mut all = STATS.lock().unwrap_or_else(PoisonError::into_inner);
    all.retain(|(v, _)| *v != view);
    all.push((view, Arc::clone(&stats)));
    stats
}

fn state() -> Json {
    let song = studio_view_link::current().map(|(key, s)| (key, s.project.tracks.len()));
    let frames: serde_json::Map<String, Json> = STATS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .map(|(view, s)| {
            (
                (*view).to_owned(),
                s.lock().unwrap_or_else(PoisonError::into_inner).json(),
            )
        })
        .collect();
    json!({
        "linked": linked(),
        "problem": studio_view_link::problem(),
        "song": song.as_ref().map(|(k, _)| k.clone()),
        "tracks": song.as_ref().map(|(_, n)| *n),
        "racks": session_ui::mixer_panel::racks_shown(),
        "frames": frames,
    })
}

/// Whether this view has an engine: attached to its process, or (in a
/// browser) the page's open.
fn linked() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    return session_engine::view_client::is_linked();
    #[cfg(target_arch = "wasm32")]
    return session_ui::web_host::hosted_is_open();
}

/// Which of the two a pane is.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shown {
    Mixer,
    Strip,
}

#[cfg(not(target_arch = "wasm32"))]
impl Shown {
    const fn name(self) -> &'static str {
        match self {
            Self::Mixer => "mixer",
            Self::Strip => "strip",
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[component]
fn MixerPane() -> Element {
    rsx! { SongPane { shown: Shown::Mixer } }
}

#[cfg(not(target_arch = "wasm32"))]
#[component]
fn StripPane() -> Element {
    rsx! { SongPane { shown: Shown::Strip } }
}

/// A pane: its view once the engine has a song, a line saying what it
/// waits for until then.
#[cfg(not(target_arch = "wasm32"))]
#[component]
fn SongPane(shown: Shown) -> Element {
    let song = studio_view_link::use_song();
    let surface = studio_view_link::surface();
    match song() {
        Some((key, session)) => rsx! {
            div { style: "position:relative; width:100%; height:100%; background:{surface};",
                // A list of one, so its key is diffed: another song mounts
                // the strips afresh (a lone keyed child is patched in place).
                for key in std::iter::once(key) {
                    Strips { key: "{key}", session: Song(session.clone()), shown }
                }
            }
        },
        None => studio_view_link::waiting(),
    }
}

/// A song's session as a prop (keyed by the pane, so never compared).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub(crate) struct Song(pub(crate) StudioSession);

#[cfg(not(target_arch = "wasm32"))]
impl PartialEq for Song {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// The mixer's widget over `session`'s strips, hosted in the pane: what
/// `session_ui::mixer_panel::Mixer` is in the desktop window, with the
/// pane in place of the window.
#[cfg(not(target_arch = "wasm32"))]
#[component]
fn Strips(session: Song, shown: Shown) -> Element {
    let session = use_context_provider(|| session.0.clone());
    use_context_provider(|| Signal::new(studio_view_link::mode_from_env()));
    let mixer = use_hook(|| {
        let rows = session
            .planner
            .plan_strips(&session.planner.raw)
            .as_slice()
            .to_vec();
        let mut links = Links::alone(rows, std::collections::HashMap::new());
        links.planner = Some(session.planner.clone());
        if shown == Shown::Strip {
            links.inspector = true;
        }
        std::rc::Rc::new(HostedMixer::new(links))
    });
    let stats = use_hook(|| pane_stats(shown.name()));
    let mount = use_hook(|| {
        let scope = InScope::here();
        let (mixer, stats) = (std::rc::Rc::clone(&mixer), Arc::clone(&stats));
        studio_view_link::with_engine_values(PaintMount::new(move |_| {
            Hosted::new(
                std::rc::Rc::clone(&mixer),
                scope.clone(),
                Arc::clone(&stats),
            )
        }))
    });
    // An edit made elsewhere (the arrangement's) comes as a track event:
    // drawn as it lands, not at the pane's next tick, so it shows in the
    // same frame as there.
    use_hook(|| {
        let wake = mount.wake_handle();
        Rc::new(session_engine::engine::on_track_event(Arc::new(
            move || {
                wake.wake();
            },
        )))
    });
    let surface = studio_view_link::surface();
    rsx! {
        {studio_view_link::studio_style()}
        div {
            class: studio_view_link::ROOT_CLASS,
            style: "position:absolute; top:0; left:0; right:0; bottom:0; overflow:hidden; \
                    background:{surface};",
            PaintPane { mount }
        }
    }
}
