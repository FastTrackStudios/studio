//! The window as a dock (docs/design/docking.md): the song's panels laid
//! out by the active screenset — Performance, Overview, Session, Mix,
//! Edit, Charts, Signal — instead of one view at a time.
//!
//! On by default; `FTS_DOCK=0` keeps the old fixed column (TopBar, views,
//! BottomBar) while the dock settles.
//!
//! Every panel here is a component of `session_ui` reading the song's
//! [`StudioSession`](session_ui::studio::StudioSession) from context, so
//! the dock sits under the song (keyed by it, as the views were).
//! Screensets persist in `utils::prefs` (`dock.screensets.v1`): read and
//! repaired at start, falling back to the built-ins; written when they
//! change.

use dioxus::prelude::*;
use dock::builtin::{self, kind};
use dock::dockspace::{Chrome, InstanceId, Place, Side};
use dock::host::{Catalog, DockHost, Kind, use_dock_provider};
use dock::screenset::{FormClass, Screensets};
use session::modes::Mode;

/// Where the screensets are kept.
const PREF: &str = "dock.screensets.v1";

/// Whether the window is a dock (`FTS_DOCK=0` turns it off).
#[must_use]
pub fn enabled() -> bool {
    std::env::var("FTS_DOCK").map_or(true, |v| v != "0")
}

/// The form class of a window: phone or not (`compact`), touched or not,
/// upright or not.
#[must_use]
pub fn class_of(compact: bool, touch: bool, (w, h): (f64, f64)) -> FormClass {
    FormClass::new(compact, touch, w <= h)
}

/// The screensets: saved ones, or the built-ins.
fn load() -> Screensets {
    utils::prefs::get_raw(PREF)
        .and_then(|json| Screensets::from_json(&json))
        .filter(|s| !s.sets.is_empty())
        .unwrap_or_else(builtin::screensets)
}

/// The dock over the current song, under the window's own bars.
#[component]
pub fn SongDock(session: session_ui::studio::StudioSession, class: FormClass) -> Element {
    use_context_provider(|| session);
    let sets = use_signal(load);
    let mut form = use_signal(|| class);
    if *form.peek() != class {
        form.set(class);
    }
    use_dock_provider(sets, form, catalog());
    // Kept as they change: a recall, a save, a panel moved or resized.
    use_effect(move || {
        if let Ok(json) = sets.read().to_json() {
            utils::prefs::set_raw(PREF, &json);
        }
    });
    rsx! {
        div { style: "position:relative; flex:1; min-height:0; min-width:0; display:flex;",
            DockHost {}
        }
    }
}

fn catalog() -> Catalog {
    let k = |kind: &'static str,
             title: &'static str,
             short: &'static str,
             home: Place,
             render: fn(InstanceId) -> Element| Kind {
        kind,
        title,
        short,
        home,
        chrome: Chrome::Bare,
        attach: None,
        render,
    };
    Catalog::new(vec![
        k(
            kind::ARRANGEMENT,
            "Arrangement",
            "ARR",
            Place::Center,
            |_| {
                rsx! { ArrangementPanel {} }
            },
        ),
        k(
            kind::MIXER,
            "Mixer",
            "MIX",
            Place::Edge(Side::Bottom),
            |_| {
                rsx! { MixerPanel {} }
            },
        ),
        k(
            kind::CHART,
            "Chart",
            "CHT",
            Place::Edge(Side::Right),
            |_| {
                rsx! { Fill { session_ui::chart_panel::Chart { paged: true } } }
            },
        ),
        k(
            kind::CHART_EDITOR,
            "Chart editor",
            "CED",
            Place::Edge(Side::Right),
            |_| {
                rsx! { Fill { session_ui::chart_editor::ChartEditor {} } }
            },
        ),
        k(
            kind::LYRICS,
            "Lyrics",
            "LYR",
            Place::Edge(Side::Right),
            |_| {
                rsx! { Fill { session_ui::lyrics_panel::LyricsPanel {} } }
            },
        ),
        k(kind::FX_ROW, "FX", "FX", Place::Edge(Side::Bottom), |_| {
            rsx! { Fill { session_ui::fx_view::FxPanel {} } }
        }),
        k(
            kind::EDITOR,
            "Editor",
            "EDT",
            Place::Edge(Side::Bottom),
            |_| {
                rsx! { Fill { session_ui::editor_view::EditorPanel {} } }
            },
        ),
        k(
            kind::TRANSPORT,
            "Transport",
            "▶",
            Place::Edge(Side::Bottom),
            |_| {
                rsx! { Fill { session_ui::transport_bar::TransportBar { big: true } } }
            },
        ),
        k(
            kind::PROGRESS,
            "Progress",
            "PRG",
            Place::Edge(Side::Top),
            |_| {
                rsx! {
                    div { style: "flex:1; min-width:0; display:flex; align-items:center; padding:4px 8px;",
                        session_ui::progress::ProgressBar { height: Some("100%".to_owned()), flat: true }
                    }
                }
            },
        ),
        k(
            kind::TOOLBAR,
            "Toolbar",
            "TB",
            Place::Edge(Side::Top),
            |_| {
                rsx! {
                    div { style: "flex:1; min-width:0; display:flex; align-items:center; padding:0 8px; \
                                  background:{dock::host::tokens::SHEET};",
                        dock::host::ScreensetBar {}
                    }
                }
            },
        ),
        k(
            kind::SETLIST,
            "Setlist",
            "SET",
            Place::Edge(Side::Left),
            |_| {
                rsx! { Fill { session_ui::setup::SetupView { on_pick: None } } }
            },
        ),
        k(
            kind::BROWSER,
            "Browser",
            "BRW",
            Place::Edge(Side::Left),
            |_| {
                rsx! { StandIn { name: "Browser", note: "the sample and FX browser moves here next" } }
            },
        ),
        k(
            kind::INSPECTOR,
            "Inspector",
            "INS",
            Place::Edge(Side::Right),
            |_| {
                rsx! { StandIn { name: "Inspector", note: "the selected track's inspector moves here next" } }
            },
        ),
        k(kind::TCP, "Tracks", "TCP", Place::Edge(Side::Left), |_| {
            rsx! { StandIn { name: "Tracks", note: "attached to an arrangement, the track headers draw in its canvas" } }
        }),
        k(
            kind::LYRIC_EDITOR,
            "Lyric editor",
            "LED",
            Place::Edge(Side::Right),
            |_| {
                rsx! { StandIn { name: "Lyric editor", note: "not on this renderer yet" } }
            },
        ),
        k(kind::RIG, "Rig", "RIG", Place::Center, |_| {
            rsx! { StandIn { name: "Rig", note: "the live rig mounts here in Studio" } }
        }),
    ])
}

/// A panel that places its content absolutely gets a sized, positioned box.
#[component]
fn Fill(children: Element) -> Element {
    rsx! {
        div { style: "position:relative; flex:1; min-width:0; min-height:0; overflow:hidden;",
            div { style: "position:absolute; inset:0; display:flex; flex-direction:column;", {children} }
        }
    }
}

/// The arrangement, its toolbar and its track headers: one canvas.
#[component]
fn ArrangementPanel() -> Element {
    let mode: Signal<Mode> = use_context();
    rsx! { Fill { session_ui::mixer_panel::DawPanels { mode: Some(mode()) } } }
}

#[component]
fn MixerPanel() -> Element {
    let mode: Signal<Mode> = use_context();
    rsx! { Fill { session_ui::mixer_panel::DawPanels { mode: Some(mode()), mixer_only: true } } }
}

/// A place held for a panel not on the dock yet, saying so.
#[component]
fn StandIn(name: &'static str, note: &'static str) -> Element {
    rsx! {
        div {
            style: "flex:1; min-width:0; min-height:0; display:flex; flex-direction:column; \
                    align-items:center; justify-content:center; gap:6px; padding:12px; \
                    color:{dock::host::tokens::INK_3}; text-align:center;",
            div { style: "font-weight:600; color:{dock::host::tokens::INK};", "{name}" }
            div { style: "font-size:12px;", "{note}" }
        }
    }
}
