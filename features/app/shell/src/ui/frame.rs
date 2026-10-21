//! The frame: the app bar on top, the sidebars down the sides, and the main
//! column with its two drawers — drawn from a [`ShellState`] and (for the
//! regions the caller doesn't fill itself) a [`PanelRegistry`].
//!
//! Blitz-first: inline styles only, no `position: fixed`, no CSS
//! transitions (CLAUDE.md, "Blitz's gaps"). A frame can nest inside another
//! frame's main area: an app frame carries the app bar and the app-level
//! flyouts, and a workspace mounts its own frame (own scope, own registry)
//! for its sidebars and drawers.

use dioxus::prelude::*;

use super::layout::{Region, ShellLayout, pref_key};
use super::registry::{Panel, PanelRegistry, Render};

const LINE: &str = "#1f1f23";
const RAIL_BG: &str = "#0b0b0e";
const INK: &str = "#e4e4e7";
const INK_DIM: &str = "#71717a";
const PICKED_BG: &str = "#0c4a6e";
const PICKED_INK: &str = "#e0f2fe";
/// A collapsed sidebar's rail (spec §2.1: 44 desktop / 56 touch; the touch
/// width comes with the size classes, spec §10 step 1).
const RAIL_W: u32 = 44;
/// The 48px hit floor for touch (spec §3.1) holds for every rail button.
const HIT: u32 = 48;

/// A scope's layout, shared by its frame and every panel inside it.
///
/// `Copy` (two signals and a `&'static str`), so panels take it out of
/// context and keep it in closures freely.
#[derive(Clone, Copy, PartialEq)]
pub struct ShellState {
    scope: &'static str,
    layout: Signal<ShellLayout>,
}

impl ShellState {
    /// The scope this layout is saved under (`signal`, `signal.gig`).
    #[must_use]
    pub const fn scope(&self) -> &'static str {
        self.scope
    }

    /// A copy of the whole layout (subscribes the caller).
    #[must_use]
    pub fn layout(&self) -> ShellLayout {
        self.layout.read().clone()
    }

    /// Whether a region is showing (subscribes the caller).
    #[must_use]
    pub fn is_open(&self, region: Region) -> bool {
        self.layout.read().is_open(region)
    }

    /// A region's size in logical px (subscribes the caller).
    #[must_use]
    pub fn size(&self, region: Region) -> u32 {
        self.layout.read().size(region)
    }

    /// The panel id a region remembers (subscribes the caller).
    #[must_use]
    pub fn active(&self, region: Region) -> Option<String> {
        self.layout.read().active(region).map(str::to_string)
    }

    /// Flip a region open or closed, and save.
    pub fn toggle(&mut self, region: Region) {
        self.update(|l| l.toggle(region));
    }

    /// Open or close a region, and save.
    pub fn set_open(&mut self, region: Region, open: bool) {
        self.update(|l| l.set_open(region, open));
    }

    /// Resize a region (clamped to its limits), and save.
    pub fn resize(&mut self, region: Region, size: u32) {
        self.update(|l| l.resize(region, size));
    }

    /// Show a panel in its region (opening the region), and save.
    pub fn show(&mut self, region: Region, id: &str) {
        self.update(|l| {
            let picked = l.set_active(region, id);
            let opened = l.set_open(region, true);
            picked || opened
        });
    }

    /// Apply an edit; write it back (and wake readers) only if it changed
    /// something — the shell must not cost a redraw per frame (audit §4.2).
    fn update(&mut self, edit: impl FnOnce(&mut ShellLayout) -> bool) {
        let mut next = self.layout.peek().clone();
        if edit(&mut next) {
            utils::prefs::store(&pref_key(self.scope), next.clone());
            self.layout.set(next);
        }
    }
}

/// The saved layout for a scope, or the default one.
#[must_use]
pub fn load_layout(scope: &str) -> ShellLayout {
    utils::prefs::load::<ShellLayout>(&pref_key(scope))
        .map(ShellLayout::sanitized)
        .unwrap_or_default()
}

/// Create the layout state for a scope and provide it to everything below
/// (the frame, and panels that open or close regions).
pub fn use_shell_state(scope: &'static str) -> ShellState {
    use_context_provider(|| ShellState {
        scope,
        layout: Signal::new(load_layout(scope)),
    })
}

/// The layout state the nearest frame above provides.
///
/// # Panics
/// Without a [`use_shell_state`] above — a panel mounted outside any shell
/// is a wiring bug, not a runtime state.
#[must_use]
pub fn use_shell() -> ShellState {
    use_context::<ShellState>()
}

/// Provide the panels this scope can show. Built once per mount.
pub fn use_panel_registry(build: impl FnOnce() -> PanelRegistry) -> PanelRegistry {
    use_context_provider(build)
}

/// The frame.
///
/// Every slot is optional. A slot given explicitly is drawn as-is: the
/// caller owns its chrome (the app's Settings flyout is its own
/// `fts_audio_ui::chrome::PanelHost`, with its own header and close). A slot
/// left out is filled from the [`PanelRegistry`] in context (if any) with
/// the shell's own region chrome: tabs when a region has several panels,
/// collapse to a rail (sidebars) or a handle (drawers).
#[component]
pub fn ShellFrame(
    /// The app bar. Only the outermost frame has one.
    #[props(default)]
    app_bar: Option<Element>,
    #[props(default)] left: Option<Element>,
    #[props(default)] right: Option<Element>,
    #[props(default)] top: Option<Element>,
    #[props(default)] bottom: Option<Element>,
    #[props(default)] main: Option<Element>,
    /// The outermost frame fills the window (`100vh`); a nested one fills
    /// the main area it is mounted in.
    #[props(default)]
    root: bool,
) -> Element {
    let registry = try_use_context::<PanelRegistry>().unwrap_or_default();
    let fill = if root {
        // The same page AppFrame drew, so Signal looks unchanged on the shell.
        "height: 100vh; background: #0a0a0a; color: #e4e4e7; font-family: sans-serif;"
    } else {
        "flex: 1; min-width: 0;"
    };
    let slot = |given: Option<Element>, region: Region| -> Option<Element> {
        given.or_else(|| registry.has(region).then(|| rsx! { RegionHost { region } }))
    };
    let left = slot(left, Region::Left);
    let right = slot(right, Region::Right);
    let top = slot(top, Region::Top);
    let bottom = slot(bottom, Region::Bottom);
    let main = slot(main, Region::Main);
    rsx! {
        div {
            "data-shell": "frame",
            style: "display: flex; flex-direction: column; min-height: 0; {fill}",
            if let Some(bar) = app_bar { {bar} }
            div { style: "display: flex; flex: 1; min-height: 0; min-width: 0;",
                if let Some(left) = left { {left} }
                div {
                    "data-shell": "main-column",
                    style: "display: flex; flex-direction: column; flex: 1; min-width: 0; min-height: 0;",
                    if let Some(top) = top { {top} }
                    div {
                        "data-shell": "main",
                        style: "display: flex; flex: 1; min-width: 0; min-height: 0;",
                        if let Some(main) = main { {main} }
                    }
                    if let Some(bottom) = bottom { {bottom} }
                }
                if let Some(right) = right { {right} }
            }
        }
    }
}

/// A region filled from the registry, with the shell's chrome around it.
#[component]
pub fn RegionHost(region: Region) -> Element {
    let registry = try_use_context::<PanelRegistry>().unwrap_or_default();
    let shell = use_shell();
    let panels: Vec<Panel> = registry.in_region(region).cloned().collect();
    let remembered = shell.active(region);
    let Some(showing) = registry.showing(region, remembered.as_deref()).cloned() else {
        return rsx! {};
    };
    let body = rsx! { PanelSlot { key: "{showing.id}", render: showing.render } };
    match region {
        Region::Main => body,
        Region::Left | Region::Right => rsx! {
            Sidebar { region, panels, showing: showing.id, body }
        },
        Region::Top | Region::Bottom => rsx! {
            Drawer { region, panels, showing: showing.id, body }
        },
    }
}

/// Mounts one panel's body in its own scope, so its hooks belong to it and
/// switching tabs remounts cleanly (the caller keys it by panel id).
#[component]
fn PanelSlot(render: Render) -> Element {
    (render.0)()
}

fn tab_style(picked: bool) -> String {
    let (bg, ink) = if picked {
        (PICKED_BG, PICKED_INK)
    } else {
        ("transparent", INK_DIM)
    };
    format!(
        "border: none; border-radius: 6px; padding: 4px 10px; min-height: 28px; \
         background: {bg}; color: {ink}; font-size: 12px; font-weight: 650; cursor: pointer;"
    )
}

const HANDLE_BTN: &str = "border: none; background: transparent; color: #a1a1aa; border-radius: 6px; \
                          min-width: 28px; min-height: 28px; font-size: 14px; cursor: pointer;";

/// Tabs, drawn only when a region holds more than one panel.
#[component]
fn Tabs(region: Region, panels: Vec<Panel>, showing: &'static str) -> Element {
    let mut shell = use_shell();
    if panels.len() < 2 {
        return rsx! {};
    }
    rsx! {
        div { style: "display: flex; gap: 4px; flex-wrap: wrap; flex: 1; min-width: 0;",
            for p in panels.iter() {
                {
                    let id = p.id;
                    rsx! {
                        button {
                            key: "{id}",
                            style: tab_style(id == showing),
                            onclick: move |_| shell.show(region, id),
                            "{p.title}"
                        }
                    }
                }
            }
        }
    }
}

/// A sidebar: the panel at its saved width with a collapse button, or —
/// collapsed — a rail with one button per panel.
#[component]
fn Sidebar(region: Region, panels: Vec<Panel>, showing: &'static str, body: Element) -> Element {
    let mut shell = use_shell();
    let left = region == Region::Left;
    let edge = if left { "border-right" } else { "border-left" };
    if !shell.is_open(region) {
        let open_glyph = if left { "›" } else { "‹" };
        return rsx! {
            div {
                "data-shell": "rail-{region.slug()}",
                style: "display: flex; flex-direction: column; align-items: center; gap: 4px; \
                        width: {RAIL_W}px; flex-shrink: 0; padding: 6px 0; {edge}: 1px solid {LINE}; \
                        background: {RAIL_BG};",
                button {
                    style: "{HANDLE_BTN} width: {RAIL_W}px; height: {HIT}px;",
                    title: "Show",
                    onclick: move |_| shell.set_open(region, true),
                    "{open_glyph}"
                }
                for p in panels.iter() {
                    {
                        let id = p.id;
                        let initial: String = p.title.chars().take(2).collect();
                        rsx! {
                            button {
                                key: "{id}",
                                style: "{HANDLE_BTN} width: {RAIL_W}px; height: {HIT}px; font-size: 11px; font-weight: 700;",
                                title: "{p.title}",
                                onclick: move |_| shell.show(region, id),
                                "{initial}"
                            }
                        }
                    }
                }
            }
        };
    }
    let width = shell.size(region);
    let close_glyph = if left { "‹" } else { "›" };
    rsx! {
        div {
            "data-shell": "{region.slug()}",
            style: "display: flex; flex-direction: column; width: {width}px; flex-shrink: 0; min-height: 0; \
                    {edge}: 1px solid {LINE};",
            div { style: "display: flex; align-items: center; gap: 6px; padding: 4px 6px; min-height: 32px;",
                if left {
                    Tabs { region, panels: panels.clone(), showing }
                    span { style: "flex: 1;" }
                    button { style: HANDLE_BTN, title: "Collapse", onclick: move |_| shell.set_open(region, false), "{close_glyph}" }
                } else {
                    button { style: HANDLE_BTN, title: "Collapse", onclick: move |_| shell.set_open(region, false), "{close_glyph}" }
                    span { style: "flex: 1;" }
                    Tabs { region, panels: panels.clone(), showing }
                }
            }
            div { style: "display: flex; flex-direction: column; flex: 1; min-height: 0;", {body} }
        }
    }
}

/// A drawer: a slim handle (title, tabs, open/close) and, when open, the
/// panel at its content height, capped at the region's size.
#[component]
fn Drawer(region: Region, panels: Vec<Panel>, showing: &'static str, body: Element) -> Element {
    let shell = use_shell();
    let open = shell.is_open(region);
    let cap = shell.size(region);
    let top = region == Region::Top;
    let edge = if top { "border-bottom" } else { "border-top" };
    rsx! {
        div {
            "data-shell": "{region.slug()}",
            style: "display: flex; flex-direction: column; flex-shrink: 0; min-width: 0; {edge}: 1px solid {LINE}; color: {INK};",
            if !top {
                DrawerHandle { region, panels: panels.clone(), showing }
            }
            if open {
                div { style: "display: flex; flex-direction: column; min-height: 0; max-height: {cap}px; overflow: hidden;", {body} }
            }
            if top {
                DrawerHandle { region, panels: panels.clone(), showing }
            }
        }
    }
}

/// A drawer's handle: open/close, and the panel's title (or its tabs).
#[component]
fn DrawerHandle(region: Region, panels: Vec<Panel>, showing: &'static str) -> Element {
    let mut shell = use_shell();
    let open = shell.is_open(region);
    let top = region == Region::Top;
    let title = panels
        .iter()
        .find(|p| p.id == showing)
        .map_or("", |p| p.title);
    // The arrow points the way the drawer will move.
    let glyph = match (top, open) {
        (true, true) | (false, false) => "▴",
        (true, false) | (false, true) => "▾",
    };
    let hint = if open { "Collapse" } else { "Show" };
    rsx! {
        div { style: "display: flex; align-items: center; gap: 6px; padding: 2px 10px; min-height: 28px;",
            button {
                style: HANDLE_BTN,
                title: "{hint}",
                onclick: move |_| shell.toggle(region),
                "{glyph}"
            }
            if panels.len() < 2 || !open {
                span { style: "font-size: 11px; color: {INK_DIM}; font-weight: 650;", "{title}" }
            }
            if open {
                Tabs { region, panels: panels.clone(), showing }
            }
        }
    }
}
