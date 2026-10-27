//! The performer screen: the shared "worship / gig" layout for the 11-inch
//! iPad in landscape (1194 x 834), mounted by Signal (rig plus playback)
//! and by Session (playback, plus the rig when a Signal peer is reachable).
//!
//! Design: the worship / gig screen spec (STA-352), build issue B1. Read
//! top to bottom it is three bands — what is playing (the now band), the
//! chart beside the setlist, and what you operate (the dock: the rig's
//! mini switches beside the transport). Reading is at the top, touching
//! at the bottom.
//!
//! ```text
//! y=0    status inset 24 (system; painted DESK)
//! y=24   app bar 48
//! y=72   perform.now      Region::Top     112
//! y=184  perform.setlist  Region::Left    320 | perform.chart  Region::Main 874   486
//! y=670  perform.dock     Region::Bottom  144 (rig 592 · 16 · transport 562)
//! y=814  home-indicator inset 20 (painted DESK, never white)
//! ```
//!
//! Two nested [`ShellFrame`]s, as the spec draws it (§6.1): the outer one
//! carries the app bar, the now band on top, the dock at the foot and the
//! inner frame in its main area; the inner one carries the setlist on the
//! left and the chart in its main area. The regions here are filled
//! explicitly, without the drawers' handles or the sidebars' tab rows: the
//! sizes above are the design, and a handle would take 28 px from the
//! chart. The panels come from the [`PanelRegistry`] in context, by the
//! ids in [`ids`], so each app registers its own bodies (Signal's
//! `TabletSetlist`, Session's own; either app's chart) and nothing is
//! forked.
//!
//! The layout is saved per app under the size class
//! [`SIZE_CLASS`] (`signal.gig.ipad-landscape-11`); a stored layout that
//! names a panel this build doesn't know loads the preset instead.

use dioxus::prelude::*;

use super::frame::{ShellFrame, use_shell, use_shell_state_with};
use super::layout::{Region, RegionLayout, ShellLayout, pref_key};
use super::registry::{Panel, PanelRegistry, Render};
use crate::tokens::{DESK, FONT, INK, MAIN, RULE, SHEET};

/// The size class this preset is for (unified shell spec §2.3).
pub const SIZE_CLASS: &str = "ipad-landscape-11";

/// Signal's performer layout scope (its saved-layout key is
/// [`pref_key`] of this).
pub const SIGNAL_SCOPE: &str = "signal.gig.ipad-landscape-11";
/// Session's performer layout scope.
pub const SESSION_SCOPE: &str = "session.gig.ipad-landscape-11";

/// The performer screen's panel ids, namespaced like the shell's `gig.*`.
pub mod ids {
    /// The now band: song, clock, NEXT, section progress. Never hides.
    pub const NOW: &str = "perform.now";
    /// The setlist with its search. Folds to a rail.
    pub const SETLIST: &str = "perform.setlist";
    /// The chart (PDF, Keyflow, Lanes).
    pub const CHART: &str = "perform.chart";
    /// The dock: the rig's switches beside the transport. Never hides.
    pub const DOCK: &str = "perform.dock";

    /// Every id, with the region it lives in.
    pub const ALL: [(&str, super::Region); 4] = [
        (NOW, super::Region::Top),
        (SETLIST, super::Region::Left),
        (CHART, super::Region::Main),
        (DOCK, super::Region::Bottom),
    ];
}

// ── The geometry (spec §3), in logical px ─────────────────────────────────

/// The whole screen: an 11-inch iPad in landscape.
pub const WIDTH: u32 = 1194;
pub const HEIGHT: u32 = 834;
/// The system's status inset, painted the desk.
pub const STATUS_INSET: u32 = 24;
/// The app bar.
pub const APP_BAR: u32 = 48;
/// The now band (title row 48, progress 44, padding).
pub const NOW_BAND: u32 = 112;
/// The dock: 8 + 60 + 8 + 60 + 8.
pub const DOCK: u32 = 144;
/// The home-indicator inset, painted the desk (never white).
pub const HOME_INSET: u32 = 20;
/// The band between the now band and the dock: setlist and chart.
pub const BODY: u32 = HEIGHT - STATUS_INSET - APP_BAR - NOW_BAND - DOCK - HOME_INSET;
/// The setlist column.
pub const SETLIST_W: u32 = 320;
/// The chart, beside the setlist.
pub const CHART_W: u32 = WIDTH - SETLIST_W;
/// The setlist folded: a rail with its chevron.
pub const RAIL_W: u32 = 56;
/// The dock's side padding, and the room between its two halves.
pub const DOCK_PAD: u32 = 12;
pub const DOCK_SPLIT: u32 = 16;
/// The dock's rows (switch tiles, transport cells) and the gap in a group.
pub const DOCK_ROW: u32 = 60;
pub const GAP: u32 = 8;
/// The rig half: five 112 px tiles, 8 apart.
pub const RIG_W: u32 = 5 * 112 + 4 * GAP;
/// The transport half: the rest.
pub const TRANSPORT_W: u32 = WIDTH - 2 * DOCK_PAD - DOCK_SPLIT - RIG_W;
/// The smallest thing a finger presses on this screen.
pub const HIT: u32 = 48;

/// The four bodies an app gives the performer screen: its own now band
/// (usually the shared [`super::NowBand`] over its progress bar), its
/// setlist, its chart and its dock.
#[derive(Clone, Copy)]
pub struct PerformerBodies {
    pub now: fn() -> Element,
    pub setlist: fn() -> Element,
    pub chart: fn() -> Element,
    pub dock: fn() -> Element,
}

/// The performer screen's panels, registered under [`ids`] in the regions
/// they live in — **the one registry function** both apps call (Signal's
/// gig mount and Session's performer view). An app never builds these
/// [`Panel`]s itself: a test reads both apps' sources to hold them to it
/// (`tests/performer_mounts.rs`).
#[must_use]
pub fn performer_registry(bodies: PerformerBodies) -> PanelRegistry {
    PanelRegistry::new()
        .with(Panel::new(ids::NOW, "Now", Region::Top, bodies.now))
        .with(Panel::new(
            ids::SETLIST,
            "Setlist",
            Region::Left,
            bodies.setlist,
        ))
        .with(Panel::new(ids::CHART, "Chart", Region::Main, bodies.chart))
        .with(Panel::new(ids::DOCK, "Dock", Region::Bottom, bodies.dock))
}

/// The layout this screen opens with, and falls back to.
#[must_use]
pub fn preset() -> ShellLayout {
    let at = |size: u32, id: &str| RegionLayout {
        open: true,
        size,
        active: Some(id.to_string()),
    };
    ShellLayout {
        left: at(SETLIST_W, ids::SETLIST),
        // Nothing lives on the right; it stays shut.
        right: RegionLayout {
            open: false,
            ..RegionLayout::initial(Region::Right)
        },
        top: at(NOW_BAND, ids::NOW),
        bottom: at(DOCK, ids::DOCK),
        moved: Vec::new(),
    }
}

/// Whether a stored layout still fits this screen: every panel it names
/// is one of [`ids::ALL`], in the region that id lives in.
#[must_use]
pub fn fits(layout: &ShellLayout) -> bool {
    Region::EDGES.iter().all(|&region| {
        layout
            .active(region)
            .is_none_or(|id| ids::ALL.iter().any(|&(k, r)| k == id && r == region))
    })
}

/// The layout to open with, from what was stored (if anything): the
/// stored one when it [`fits`], else the [`preset`]. The now band and the
/// dock never hide, whatever was saved.
#[must_use]
pub fn performer_layout(stored: Option<ShellLayout>) -> ShellLayout {
    let mut layout = match stored {
        Some(l) if fits(&l) => l.sanitized(),
        _ => preset(),
    };
    layout.set_open(Region::Top, true);
    layout.set_open(Region::Bottom, true);
    layout
}

/// The stored layout for a performer scope, read the way
/// [`performer_layout`] wants it.
#[must_use]
pub fn load_performer_layout(scope: &str) -> ShellLayout {
    performer_layout(utils::prefs::load::<ShellLayout>(&pref_key(scope)))
}

/// The performer screen.
///
/// Provide a [`PanelRegistry`] with the four [`ids`] above it
/// ([`super::use_panel_registry`]) and the app's playback
/// ([`super::provide_playback`]); a panel left out is drawn empty (Session
/// without a rig registers its dock with no rig half).
#[component]
pub fn PerformerScreen(
    /// The layout scope: [`SIGNAL_SCOPE`] or [`SESSION_SCOPE`].
    scope: &'static str,
    /// The app bar's contents (48 px), e.g. Signal's iPad `TopBar`.
    #[props(default)]
    app_bar: Option<Element>,
    /// The status and home-indicator insets, painted the desk. The iPad's
    /// are 24 and 20; a desktop window passes 0.
    #[props(default = STATUS_INSET)]
    inset_top: u32,
    #[props(default = HOME_INSET)] inset_bottom: u32,
) -> Element {
    use_shell_state_with(scope, load_performer_layout);
    // The app's panels, read by id here; the frames below see an empty
    // registry, so they draw only the slots given (no tab rows, rails or
    // drawer handles of their own).
    let panels = try_use_context::<PanelRegistry>().unwrap_or_default();
    use_context_provider(|| PerformPanels(panels));
    rsx! { Frames { inset_top, inset_bottom, app_bar } }
}

/// The app's [`PanelRegistry`], as the performer screen saw it: the frames
/// inside it get an empty one.
#[derive(Clone, PartialEq)]
struct PerformPanels(PanelRegistry);

#[component]
fn Frames(inset_top: u32, inset_bottom: u32, app_bar: Option<Element>) -> Element {
    use_context_provider(PanelRegistry::new);
    let bar = rsx! {
        div {
            "data-perform": "app-bar",
            style: "display: flex; align-items: center; flex-shrink: 0; height: {APP_BAR}px; min-width: 0; \
                    border-bottom: 1px solid {RULE}; background: {SHEET}; box-sizing: border-box;",
            if let Some(bar) = app_bar { {bar} }
        }
    };
    let top = rsx! {
        div {
            "data-perform": "now",
            style: "display: flex; flex-direction: column; flex-shrink: 0; height: {NOW_BAND}px; min-width: 0; \
                    border-bottom: 1px solid {RULE}; background: {SHEET}; box-sizing: border-box;",
            PerformPanel { id: ids::NOW }
        }
    };
    let bottom = rsx! {
        div {
            "data-perform": "dock",
            style: "display: flex; flex-shrink: 0; height: {DOCK}px; min-width: 0; \
                    border-top: 1px solid {RULE}; background: {SHEET}; box-sizing: border-box;",
            PerformPanel { id: ids::DOCK }
        }
    };
    let body = rsx! {
        ShellFrame {
            left: rsx! { SetlistColumn {} },
            main: rsx! {
                div {
                    "data-perform": "chart",
                    style: "position: relative; display: flex; flex: 1; min-width: 0; min-height: 0; background: {MAIN};",
                    PerformPanel { id: ids::CHART }
                }
            },
        }
    };
    rsx! {
        div {
            "data-perform": "screen",
            style: "display: flex; flex-direction: column; width: 100vw; height: 100vh; min-height: 0; \
                    background: {DESK}; color: {INK}; font-family: {FONT};",
            div { "data-perform": "status-inset", style: "flex-shrink: 0; height: {inset_top}px; background: {DESK};" }
            ShellFrame { app_bar: bar, top, main: body, bottom }
            div { "data-perform": "home-inset", style: "flex-shrink: 0; height: {inset_bottom}px; background: {DESK};" }
        }
    }
}

/// The setlist's column at its saved width, or — folded — a rail with a
/// chevron that opens it again (the chart takes the room).
#[component]
fn SetlistColumn() -> Element {
    let mut shell = use_shell();
    if !shell.is_open(Region::Left) {
        return rsx! {
            div {
                "data-perform": "setlist-rail",
                style: "display: flex; flex-direction: column; align-items: center; width: {RAIL_W}px; flex-shrink: 0; \
                        padding-top: 8px; border-right: 1px solid {RULE}; background: {SHEET};",
                button {
                    "aria-label": "Show the setlist",
                    style: "{CHEVRON} width: {HIT}px; height: {HIT}px;",
                    onclick: move |_| shell.set_open(Region::Left, true),
                    "›"
                }
            }
        };
    }
    let width = shell.size(Region::Left);
    rsx! {
        div {
            "data-perform": "setlist",
            style: "position: relative; display: flex; flex-direction: column; width: {width}px; flex-shrink: 0; \
                    min-height: 0; border-right: 1px solid {RULE}; background: {SHEET};",
            PerformPanel { id: ids::SETLIST }
        }
    }
}

const CHEVRON: &str = "border: none; border-radius: 8px; background: rgba(255,255,255,0.06); \
                       color: #e4e4e7; font-size: 22px; font-weight: 600; cursor: pointer;";

/// A registered panel's body by id, or nothing when none is registered.
#[component]
fn PerformPanel(id: &'static str) -> Element {
    let registry = try_use_context::<PerformPanels>()
        .map(|p| p.0)
        .unwrap_or_default();
    match registry.get(id).map(|p| p.render) {
        Some(render) => rsx! { PanelBody { key: "{id}", render } },
        None => rsx! {},
    }
}

/// Mounts one panel's body in its own scope, so its hooks are its own.
#[component]
fn PanelBody(render: Render) -> Element {
    (render.0)()
}

/// The dock (`perform.dock`): the rig's switches beside the transport, two
/// halves in one region (the spec's composite panel, §6.1).
///
/// Without a rig (Session with no Signal ever paired) the transport takes
/// the whole width.
#[component]
pub fn PerformDock(
    /// The rig half: `TouchSwitches` at its `Gig` size, or `None`.
    #[props(default)]
    rig: Option<Element>,
    /// The transport half: Session's `TransportControlBar` (`wide_play`)
    /// over [`super::TransportNudgeRow`], or a [`super::FollowCard`] while
    /// following a peer.
    transport: Element,
) -> Element {
    rsx! {
        div {
            style: "display: flex; flex: 1; min-width: 0; gap: {DOCK_SPLIT}px; padding: {GAP}px {DOCK_PAD}px; box-sizing: border-box;",
            if let Some(rig) = rig {
                div { "data-perform": "rig", style: "display: flex; flex-direction: column; width: {RIG_W}px; flex-shrink: 0; min-width: 0;", {rig} }
            }
            div { "data-perform": "transport", style: "display: flex; flex-direction: column; gap: {GAP}px; flex: 1; min-width: 0;", {transport} }
        }
    }
}
