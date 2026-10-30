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

/// Panels the host app adds to the window's dock, or puts in place of one
/// of its own by kind: Studio's live rig (`rig`), which Session cannot
/// mount itself. Provided above [`Embedded`](super::Embedded).
#[derive(Clone, Default)]
pub struct HostPanels {
    /// Kinds added to the catalog, or put in place of one by kind.
    pub kinds: Vec<Kind>,
    /// Drawn around the dock, inside the song's context: where the host
    /// provides what its panels share (Studio: the rig's connection, its
    /// gig state and playback). Renders `children` as given when `None`.
    pub scope: Option<fn(Element) -> Element>,
}

/// What the View menu asks of the dock, and the way to ask it — for a menu
/// in the window's own process (Studio's macOS menu bar). The same queue
/// the View actions (`SESSION_SCREENSET`, …) fill.
pub use session_engine::actions::{DockRequest, request_dock};

/// Runs a window action behind the window, in order — for the View menu's
/// song items (`SESSION_SONG`).
pub fn run_action(id: &'static str, args: String) {
    session_engine::actions::off_the_window(id, args);
}

/// A dock control pressed: its action run through the Action List
/// (`session_ui::window::press`), which comes back to the dock as a
/// request (`apply`).
fn press(id: &str, args: &serde_json::Value) {
    let _ = session_ui::window::press(id, args);
}

/// Where the screensets are kept.
const PREF: &str = "dock.screensets.v1";

/// Whether the window is a dock (`FTS_DOCK=0` turns it off).
#[must_use]
pub fn enabled() -> bool {
    std::env::var("FTS_DOCK").map_or(true, |v| v != "0")
}

/// The form class of a window: phone or not (`compact`), touched or not,
/// upright or not. `FTS_TOUCH=1` makes a desk window a tablet, to see the
/// iPad layouts without an iPad.
#[must_use]
pub fn class_of(compact: bool, touch: bool, (w, h): (f64, f64)) -> FormClass {
    let touch = touch || std::env::var("FTS_TOUCH").is_ok_and(|v| v == "1");
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
    let host = try_use_context::<HostPanels>().unwrap_or_default();
    let scope = host.scope;
    let dock = use_dock_provider(sets, form, catalog(host));
    // While it is up, the pane keys (`x`, `e`, `f`, `b`) and actions
    // toggle its panels.
    use_hook(|| session_engine::actions::set_docked(true));
    use_drop(|| session_engine::actions::set_docked(false));
    // The top bar's navigator button, lit while the set is shown: the
    // navigator pin follows the dock's Set panel.
    let pins = try_use_context::<session_ui::shell::Pins>();
    use_effect(move || {
        let shown = dock.shows(kind::SET);
        if let Some(mut navigator) = pins.map(|p| p.navigator)
            && *navigator.peek() != shown
        {
            navigator.set(shown);
        }
    });
    // The View actions (SESSION_SCREENSET, …: the menu bar, keys, a model
    // over MCP), taken up within a tenth of a second.
    use_future(move || async move {
        loop {
            for request in session_engine::actions::take_dock_requests() {
                apply(dock, sets, *form.peek(), request);
            }
            architect::platform::sleep(std::time::Duration::from_millis(100)).await;
        }
    });
    // Kept as they change: a recall, a save, a panel moved or resized.
    use_effect(move || {
        if let Ok(json) = sets.read().to_json() {
            utils::prefs::set_raw(PREF, &json);
        }
    });
    let dock = rsx! { DockHost {} };
    let dock = match scope {
        Some(around) => around(dock),
        None => dock,
    };
    rsx! {
        div { style: "position:relative; flex:1; min-height:0; min-width:0; display:flex; flex-direction:column;",
            div { style: "position:relative; flex:1; min-height:0; min-width:0; display:flex;",
                {dock}
            }
            // The window's bar, the whole width: a phone has its dock bar.
            if !class.is_phone() {
                DockBar {}
            }
        }
    }
}

/// The panels the window's bar toggles: what is reached for all the time.
const BAR_PANELS: [(&str, &str); 6] = [
    (kind::MIXER, "Mixer"),
    (kind::EDITOR, "Editor"),
    (kind::FX_ROW, "FX"),
    (kind::CHART, "Chart"),
    (kind::LYRICS, "Lyrics"),
    (kind::BROWSER, "Browser"),
];

/// Along the foot of the window, the whole width (under the side docks
/// too): the common panels a press away, lit while shown; the transport;
/// the shown panels' own switches (the arrangement's inspector, Quantize
/// and lock; the mixer's folders and FX racks); what is running, the
/// phase and the modes; the screensets and Arrange. What the old bottom
/// bar held, for the screens with no menu bar (an iPad) as much as a desk.
#[component]
fn DockBar() -> Element {
    use dock::host::tokens::{INK, INK_3, PRIMARY, RULE, SHEET, SHEET_2};
    let dock = dock::host::use_dock();
    let touch = (dock.class)() != FormClass::Desk;
    let height = if touch { 64 } else { 56 };
    let set_name = dock
        .sets
        .read()
        .active_set()
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "Screensets".to_owned());
    let arranging = dock.arranging();
    let mode = try_use_context::<Signal<Mode>>();
    let (arrangement, mixer) = (dock.shows(kind::ARRANGEMENT), dock.shows(kind::MIXER));
    let button = move |on: bool| {
        format!(
            "flex:none; min-width:{w}px; height:{h}px; display:flex; flex-direction:column; \
             align-items:center; justify-content:center; gap:2px; padding:0 8px; border:none; \
             border-radius:8px; background:{bg}; color:{fg}; font-size:10px; font-weight:600;",
            w = if touch { 56 } else { 48 },
            h = height - 12,
            bg = if on { SHEET_2 } else { "transparent" },
            fg = if on { INK } else { INK_3 },
        )
    };
    let arrange_style = format!(
        "{}{}",
        button(arranging),
        if arranging {
            format!("background:{PRIMARY};")
        } else {
            String::new()
        }
    );
    rsx! {
        div {
            style: "flex:0 0 {height}px; height:{height}px; display:flex; align-items:center; gap:4px; \
                    padding:0 8px; background:{SHEET}; border-top:1px solid {RULE}; min-width:0;",
            for (kind, label) in BAR_PANELS {
                button {
                    key: "{kind}",
                    style: "{button(dock.shows(kind))}",
                    title: "{label}",
                    // `SESSION_PANEL` toggles it (`apply`), so `watch`
                    // and the CLI see the press.
                    onclick: move |_| {
                        press("SESSION_PANEL", &serde_json::json!({ "kind": kind }));
                    },
                    BarIcon { kind }
                    "{label}"
                }
            }
            div { style: "flex:none; width:1px; height:{height - 20}px; background:{RULE}; margin:0 4px;" }
            div { style: "position:relative; flex:1 1 0; min-width:0; height:100%; display:flex; align-items:center; overflow:hidden;",
                session_ui::transport_bar::TransportBar { big: touch }
            }
            // The bottom bar's own parts, flat and the bar's full height
            // as they are there; their pickers open upward, over the dock
            // (so the bar does not clip).
            div { style: "position:relative; flex:none; height:100%; display:flex; align-items:stretch;",
                if arrangement || mixer {
                    div { style: "flex:none; width:1px; margin:10px 4px; background:{RULE};" }
                    session_ui::shell::PanelControls { arrangement, mixer }
                }
                session_ui::shell::Jobs {}
                if let Some(mode) = mode {
                    session_ui::shell::PhaseSwitch { mode }
                    session_ui::shell::Modes { mode }
                }
            }
            div { style: "flex:none; width:1px; height:{height - 20}px; background:{RULE}; margin:0 4px;" }
            button {
                style: "{button(true)} min-width:96px;",
                title: "Screensets and panels",
                onclick: move |_| dock.show_menu(),
                lucide_dioxus::LayoutGrid { size: 18, color: INK }
                "{set_name}"
            }
            button {
                style: "{arrange_style}",
                title: "Arrange panels (or hold W)",
                onclick: move |_| press("SESSION_ARRANGE", &serde_json::Value::Null),
                lucide_dioxus::Move { size: 18, color: if arranging { INK } else { INK_3 } }
                "Arrange"
            }
        }
    }
}

/// A bar button's icon.
#[component]
fn BarIcon(kind: &'static str) -> Element {
    let color = dock::host::tokens::INK;
    match kind {
        kind::MIXER => rsx! { lucide_dioxus::SlidersVertical { size: 18, color } },
        kind::EDITOR => rsx! { lucide_dioxus::AudioWaveform { size: 18, color } },
        kind::FX_ROW => rsx! { lucide_dioxus::AudioLines { size: 18, color } },
        kind::CHART => rsx! { lucide_dioxus::FileMusic { size: 18, color } },
        kind::LYRICS => rsx! { lucide_dioxus::MicVocal { size: 18, color } },
        _ => rsx! { lucide_dioxus::FolderOpen { size: 18, color } },
    }
}

/// Carries out one View action on the dock.
fn apply(
    dock: dock::host::Dock,
    mut sets: Signal<Screensets>,
    class: FormClass,
    request: session_engine::actions::DockRequest,
) {
    use session_engine::actions::DockRequest;
    match request {
        DockRequest::Recall(which) => {
            let found = {
                let sets = sets.peek();
                let by_slot = which.parse::<u8>().ok().and_then(|n| sets.in_slot(n));
                by_slot
                    .or_else(|| {
                        sets.sets
                            .iter()
                            .find(|s| s.name.eq_ignore_ascii_case(&which))
                    })
                    .map(|s| s.id)
            };
            match found {
                Some(id) => dock.recall(id),
                None => tracing::warn!(screenset = %which, "no such screenset to recall"),
            }
        }
        DockRequest::Save => {
            sets.write().save(class);
        }
        DockRequest::SaveAs(name) => {
            let mut sets = sets.write();
            let name = name.unwrap_or_else(|| format!("Screenset {}", sets.sets.len() + 1));
            sets.save_as(name, class);
        }
        DockRequest::Arrange => dock.toggle_arrange(),
        DockRequest::ToggleDocks => dock.toggle_docks(),
        DockRequest::TogglePanel(kind) => {
            let was_shown = dock.shows(&kind);
            let opened = dock.toggle_kind(&kind);
            if !was_shown && !opened {
                tracing::warn!(panel.kind = %kind, "no such panel to open");
            }
        }
    }
}

fn catalog(host: HostPanels) -> Catalog {
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
    let mut kinds = vec![
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
                rsx! { TransportPanel {} }
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
                        session_ui::progress::ProgressBar { height: Some("40px".to_owned()), flat: true }
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
        k(kind::SET, "Set", "SET", Place::Edge(Side::Left), |_| {
            rsx! { SetPanel {} }
        }),
        k(
            kind::VERSIONS,
            "Versions",
            "VER",
            Place::Edge(Side::Right),
            |_| {
                rsx! {
                    div { style: "flex:1; min-width:0; display:flex; align-items:flex-start; padding:8px;",
                        session_ui::versions::Versions {}
                    }
                }
            },
        ),
        k(
            kind::COLLAB,
            "Collaborate",
            "COL",
            Place::Edge(Side::Right),
            |_| {
                rsx! {
                    div { style: "flex:1; min-width:0; display:flex; flex-wrap:wrap; align-items:flex-start; \
                                  gap:8px; padding:8px;",
                        session_ui::collab_bar::CollabBar {}
                        session_ui::shell::AudioBadge { density: session_ui::shell::Density::Full }
                    }
                }
            },
        ),
        k(
            kind::SETLIST,
            "Setlist",
            "LST",
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
                rsx! { Fill { session_ui::browser::BrowserBody {} } }
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
    ];
    for extra in host.kinds {
        match kinds.iter_mut().find(|k| k.kind == extra.kind) {
            Some(own) => *own = extra,
            None => kinds.push(extra),
        }
    }
    Catalog::new(kinds)
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

/// The set as a vertical progress bar (Session's navigator): a song
/// pressed is gone to, as its tab does.
#[component]
fn SetPanel() -> Element {
    rsx! {
        Fill {
            session_ui::navigator::Navigator {
                on_pick: move |index: usize| {
                    session_engine::actions::off_the_window(
                        "SESSION_SONG",
                        format!(r#"{{"to":"{}"}}"#, index + 1),
                    );
                },
            }
        }
    }
}

/// The transport: the wide bar, or on a phone the compact buttons the
/// small-screen layout had (the wide bar's clock and pills do not fit).
#[component]
fn TransportPanel() -> Element {
    let phone = (dock::host::use_dock().class)().is_phone();
    if phone {
        return rsx! {
            div {
                style: "flex:1; min-width:0; display:flex; flex-direction:column; justify-content:center; \
                        padding:6px 8px; background:{dock::host::tokens::SHEET};",
                session_ui::progress::TransportButtons { compact: true, height: 48 }
            }
        };
    }
    rsx! { Fill { session_ui::transport_bar::TransportBar { big: true } } }
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
