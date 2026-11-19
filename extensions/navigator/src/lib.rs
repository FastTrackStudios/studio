//! `studio.navigator`: Studio's song progress bar and set navigator in
//! Architect panes.
//!
//! Nothing is redrawn: the panes are a Studio window on Studio's engine
//! (`studio_view_link`) showing `session_ui::progress::ProgressBar` (the
//! bar across the top of `studio-desktop`'s window) and
//! `session_ui::navigator::Navigator` (its left column: the set as a
//! vertical progress bar), over the set the engine has.

use architect_extensions::linked::{Builder, Extension};
use dioxus::prelude::*;
use serde_json::json;
use session_engine::setlist::Setlist;
use session_ui::song_model::StudioSession;

pub const MANIFEST: &str = include_str!("../extension.toml");

pub struct Navigator;

impl Extension for Navigator {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        ext.view("progress", ProgressPane)
            .view("navigator", NavigatorPane)
            .view("songs", SongsPane)
            .action("STUDIO_NAVIGATOR_STATE", |_| async {
                let set = set();
                Ok(json!({
                    "linked": studio_view_link::current().is_some(),
                    "problem": studio_view_link::problem(),
                    "songs": set.as_ref().map(|s| s.songs.len()),
                    "at": set.as_ref().map(|s| s.at),
                    "pending": set.as_ref().map(|s| s.pending.clone()),
                }))
            });
    }
}

/// The set the engine has, as this window shows it.
fn set() -> Option<Setlist> {
    session_engine::view_client::peek_set(|view| view.setlist.clone())
}

/// A song's session as a prop (keyed by the pane, so never compared).
#[derive(Clone)]
struct Song(StudioSession);

impl PartialEq for Song {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// The song's progress bar, filling its pane.
#[component]
fn ProgressPane() -> Element {
    let song = studio_view_link::use_song();
    let surface = studio_view_link::surface();
    match song() {
        Some((key, session)) => rsx! {
            div { style: "position:relative; width:100%; height:100%; background:{surface}; \
                         padding:0 8px 6px; box-sizing:border-box;",
                // A list of one, so its key is diffed: another song mounts
                // the bar afresh (a lone keyed child is patched in place).
                for key in std::iter::once(key) {
                    Bar { key: "{key}", session: Song(session.clone()) }
                }
            }
        },
        None => studio_view_link::waiting(),
    }
}

#[component]
fn Bar(session: Song) -> Element {
    use_context_provider(|| session.0.clone());
    rsx! {
        // A height in pixels: the bar sizes itself, and a percentage of a
        // pane it does not fill by layout comes to nothing.
        session_ui::progress::ProgressBar { height: "44px".to_owned() }
    }
}

/// The set in context for the components under it (`Signal<Setlist>`, as
/// `studio-desktop`'s window keeps it): read from the engine a few times a
/// second.
fn use_set() -> Signal<Setlist> {
    studio_view_link::attach();
    let mut shown = use_context_provider(|| Signal::new(set().unwrap_or_default()));
    use_future(move || async move {
        loop {
            futures_timer::Delay::new(std::time::Duration::from_millis(200)).await;
            if let Some(now) = set()
                && *shown.peek() != now
            {
                shown.set(now);
            }
        }
    });
    shown
}

/// The set across a bar: a tab per song sharing its width, the current one
/// raised, each with its progress along its foot (`studio-desktop`'s top
/// bar). A tab pressed goes to that song: every Studio view follows.
#[component]
fn SongsPane() -> Element {
    let _shown = use_set();
    let surface = studio_view_link::surface();
    rsx! {
        div { style: "position:relative; width:100%; height:100%; min-height:36px; background:{surface}; \
                     display:flex; align-items:stretch;",
            session_ui::shell::SongTabs {
                on_pick: move |index: usize| {
                    let _ = session_engine::view_client::ask::pick(index);
                },
                max_shown: 0,
            }
            Hearing {}
        }
    }
}

/// The set's navigator, filling its pane: a song pressed gone to.
#[component]
fn NavigatorPane() -> Element {
    let shown = use_set();
    let surface = studio_view_link::surface();
    if shown.read().songs.is_empty() && shown.read().pending.is_empty() {
        return studio_view_link::waiting();
    }
    rsx! {
        div { style: "position:relative; width:100%; height:100%; background:{surface};",
            session_ui::navigator::Navigator {
                on_pick: move |index: usize| {
                    let _ = session_engine::view_client::ask::pick(index);
                },
            }
        }
    }
}

/// The bar's right end: how the set is heard, its stems or each song's
/// stereo reference (the engine's `SESSION_HEAR`, every song at once).
#[component]
fn Hearing() -> Element {
    // What it is now, asked of the engine off the window's thread.
    let mut by = use_signal(|| None::<String>);
    let ask = move |set: Option<&'static str>| {
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("hear".to_owned())
            .spawn(move || {
                let args = set.map_or_else(|| json!({}), |by| json!({ "by": by }));
                let said = studio_view_link::run("SESSION_HEAR", &args).ok();
                let now = said.and_then(|v| match v {
                    serde_json::Value::String(text) => {
                        serde_json::from_str::<serde_json::Value>(&text).ok()
                    }
                    other => Some(other),
                });
                let _ = tx.send(now.and_then(|v| v.get("by")?.as_str().map(str::to_owned)));
            });
        spawn(async move {
            loop {
                futures_timer::Delay::new(std::time::Duration::from_millis(40)).await;
                match rx.try_recv() {
                    Ok(now) => {
                        if now.is_some() {
                            by.set(now);
                        }
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                }
            }
        });
    };
    // Asked again every few seconds: the engine may not be linked yet at
    // first, and a script or another window may change it.
    use_future(move || async move {
        loop {
            ask(None);
            futures_timer::Delay::new(std::time::Duration::from_secs(3)).await;
        }
    });
    let now = by();
    let part = |label: &'static str, value: &'static str, title: &'static str| {
        let on = now.as_deref() == Some(value);
        let (fill, ink) = if on {
            ("#26292f", "#e6e8eb")
        } else {
            ("transparent", "#8b9099")
        };
        rsx! {
            button {
                title,
                style: "flex:none; height:22px; padding:0 10px; border:none; border-radius:5px; \
                        background:{fill}; color:{ink}; font-size:11px; font-weight:600; cursor:pointer;",
                onclick: move |_| ask(Some(value)),
                "{label}"
            }
        }
    };
    rsx! {
        div {
            style: "flex:none; display:flex; align-items:center; gap:2px; padding:0 10px; \
                    border-left:1px solid #23262b;",
            {part("Stems", "stems", "Hear every song by its multitracks")}
            {part("Reference", "reference", "Hear every song by its stereo reference: far less to render, everything still in time")}
        }
    }
}
