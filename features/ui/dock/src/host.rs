//! The dock host on Blitz: draws a [`Dockspace`] and lets the user
//! rearrange it (docs/design/docking.md).
//!
//! - **Inline styles only**, nothing Blitz drops: no `position: fixed`
//!   (every overlay is absolute against the host's root), no
//!   `overflow: auto`, no drag-and-drop events (tabs, dividers and floating
//!   windows drag on pointer events, measured with `get_client_rect`).
//! - **Context, not globals**: [`use_dock_provider`] puts a [`Dock`] in
//!   context, so two windows (or a plugin editor) each host their own.
//! - **A panel is a [`Kind`]'s render function** given its
//!   [`InstanceId`]; it reads its own instance with [`use_panel`] and keeps
//!   its view state in [`Panel::state`] through [`Dock::set_state`].
//!
//! On a phone ([`FormClass::is_phone`]) the edge docks and floating windows
//! do not share the screen: the centre fills it, and each docked panel is a
//! button on a bar (along the bottom upright, down the side sideways) that
//! opens it as a sheet.

use std::collections::HashMap;
use std::rc::Rc;

use dioxus::prelude::*;

use crate::dockspace::{
    AreaRef, Axis, Chrome, Corners, Dockspace, FloatId, InstanceId, Node, Panel, Place, Rect,
    Share, Shown, Side, SplitId, Stack, StackId,
};
use crate::hit;
use crate::screenset::{FormClass, ScreensetId, Screensets};

/// The shell's colours (docs/design/unified-shell/spec.md §9, Signal Touch).
pub mod tokens {
    pub const DESK: &str = "#09090b";
    pub const SHEET: &str = "#0e0e11";
    pub const SHEET_2: &str = "#16161a";
    pub const RULE: &str = "#2b2b31";
    pub const INK: &str = "#e4e4e7";
    pub const INK_3: &str = "#80808a";
    pub const FOCUS: &str = "#bfdbfe";
    pub const PRIMARY: &str = "#2563eb";
    pub const DROP_WASH: &str = "rgba(191,219,254,0.12)";
    pub const SCRIM: &str = "rgba(0,0,0,0.55)";
}

/// The divider's visible thickness, px.
const DIVIDER: f32 = 4.0;
/// A tab strip's height, px.
const TABS_H: f32 = 30.0;
/// How far a pressed tab moves before it is dragged, px.
const DRAG_SLOP: f32 = 6.0;
/// A collapsed dock's rail, px.
const RAIL: f32 = 36.0;
/// The phone's dock bar, px (48 hit + padding).
const PHONE_BAR: f32 = 56.0;

/// What a panel kind is and how it draws.
#[derive(Clone, Copy)]
pub struct Kind {
    /// The [`PanelKind`](crate::dockspace::PanelKind) string.
    pub kind: &'static str,
    /// The tab's label.
    pub title: &'static str,
    /// A rail's label: two or three letters.
    pub short: &'static str,
    /// Where a new one opens.
    pub home: Place,
    /// Tabs over it, or none (toolbars, the transport).
    pub chrome: Chrome,
    /// The kind it attaches beside ("tcp" beside "arrangement"), on which
    /// side and how thick.
    pub attach: Option<(&'static str, Side, f32)>,
    /// Draws instance `id`. Return a component (`rsx! { Mixer { id } }`) so
    /// the panel's hooks are its own.
    pub render: fn(InstanceId) -> Element,
}

impl PartialEq for Kind {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

/// The kinds an app offers.
#[derive(Clone)]
pub struct Catalog(Rc<Vec<Kind>>);

impl PartialEq for Catalog {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Catalog {
    #[must_use]
    pub fn new(kinds: Vec<Kind>) -> Self {
        Self(Rc::new(kinds))
    }

    #[must_use]
    pub fn get(&self, kind: &str) -> Option<&Kind> {
        self.0.iter().find(|k| k.kind == kind)
    }

    pub fn kinds(&self) -> impl Iterator<Item = &Kind> {
        self.0.iter()
    }
}

/// Something the host measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Mount {
    Host,
    Stack(StackId),
    Split(SplitId),
}

/// A drag in progress.
#[derive(Debug, Clone, PartialEq)]
enum Drag {
    /// A tab pressed, not yet moved far enough to drag.
    Pressed { id: InstanceId, x: f32, y: f32 },
    /// A tab being dragged; `None` until the host has been measured.
    Panel {
        id: InstanceId,
        geometry: Option<(Rect, Vec<(StackId, Rect)>)>,
        at: Option<hit::Target>,
    },
    /// A divider: the split's first half is the pointer's offset from
    /// `origin` along `axis`, out of `length`.
    Split {
        id: SplitId,
        axis: Axis,
        share: Share,
        origin: f32,
        length: f32,
    },
    /// An edge dock's divider, in a host at `host`.
    Edge { side: Side, host: Rect },
    /// A floating window's title bar, grabbed `dx, dy` from its corner.
    Float {
        id: FloatId,
        host: Rect,
        dx: f32,
        dy: f32,
    },
    /// A floating window's resize corner.
    FloatSize { id: FloatId, host: Rect },
}

/// A menu on show.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Menu {
    /// What can be done with a panel.
    Panel(InstanceId),
    /// Which panel to open.
    Add,
}

/// The dock host's state, in context.
#[derive(Clone, Copy)]
pub struct Dock {
    pub sets: Signal<Screensets>,
    pub class: Signal<FormClass>,
    catalog: CopyValue<Catalog>,
    drag: Signal<Option<Drag>>,
    menu: Signal<Option<Menu>>,
    sheet: Signal<Option<InstanceId>>,
    mounts: CopyValue<HashMap<Mount, Rc<MountedData>>>,
    /// The arrange key is held: every panel can be grabbed and dragged.
    held: Signal<bool>,
    /// Arrange is turned on (a touch screen has no key to hold).
    latched: Signal<bool>,
    key: CopyValue<ArrangeKey>,
}

/// The key held to arrange panels: while it is down, every panel shows
/// its name and can be dragged anywhere (or right-clicked for its menu).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArrangeKey {
    pub code: Code,
    pub shift: bool,
}

impl Default for ArrangeKey {
    /// Hold W.
    fn default() -> Self {
        Self {
            code: Code::KeyW,
            shift: false,
        }
    }
}

/// Puts a [`Dock`] over `sets` in context for the [`DockHost`] below.
pub fn use_dock_provider(
    sets: Signal<Screensets>,
    class: Signal<FormClass>,
    catalog: Catalog,
) -> Dock {
    use_context_provider(move || Dock {
        sets,
        class,
        catalog: CopyValue::new(catalog),
        drag: Signal::new(None),
        menu: Signal::new(None),
        sheet: Signal::new(None),
        mounts: CopyValue::new(HashMap::new()),
        held: Signal::new(false),
        latched: Signal::new(false),
        key: CopyValue::new(ArrangeKey::default()),
    })
}

/// The [`Dock`] a [`use_dock_provider`] above put in context.
#[must_use]
pub fn use_dock() -> Dock {
    use_context()
}

/// The panel instance this component draws (inside a [`Kind::render`]).
#[must_use]
pub fn use_panel() -> InstanceId {
    use_context::<ThisPanel>().0
}

#[derive(Clone, Copy)]
struct ThisPanel(InstanceId);

impl Dock {
    /// The layout on screen.
    #[must_use]
    pub fn space(&self) -> Dockspace {
        let class = (self.class)();
        self.sets.read().shown(class).cloned().unwrap_or_default()
    }

    /// Changes the layout on screen.
    pub fn edit<R>(&self, f: impl FnOnce(&mut Dockspace) -> R) -> R {
        let class = *self.class.peek();
        let mut sets = self.sets;
        let mut sets = sets.write();
        f(sets.current(class))
    }

    /// True while panels can be grabbed: the arrange key is held or
    /// Arrange is on. A held key lets go the moment typing starts.
    #[must_use]
    pub fn arranging(&self) -> bool {
        ((self.held)() && utils::keyboard::hotkeys_on()) || (self.latched)()
    }

    /// Holds or lets go of arranging, for an app that reads the keyboard
    /// itself (the Session window reads winit's keys, not the DOM's).
    /// Ignored while text is being typed (`utils::keyboard::typing`).
    pub fn set_arranging(&self, on: bool) {
        let on = on && utils::keyboard::hotkeys_on();
        let mut held = self.held;
        if *held.peek() != on {
            held.set(on);
        }
    }

    /// Turns Arrange on or off (the button for touch screens).
    pub fn toggle_arrange(&self) {
        let mut latched = self.latched;
        let on = !*latched.peek();
        latched.set(on);
    }

    /// Which key is held to arrange.
    pub fn set_arrange_key(&self, key: ArrangeKey) {
        let mut k = self.key;
        k.set(key);
    }

    /// Recalls screenset `id`.
    pub fn recall(&self, id: ScreensetId) {
        let mut sets = self.sets;
        sets.write().recall(id);
    }

    /// Panel `id`'s saved view state ("" when it has none).
    #[must_use]
    pub fn state(&self, id: InstanceId) -> String {
        let class = *self.class.peek();
        self.sets
            .peek()
            .shown(class)
            .and_then(|d| d.panel(id))
            .map(|p| p.state.clone())
            .unwrap_or_default()
    }

    /// Keeps panel `id`'s view state. Write it when a gesture ends, not on
    /// every frame of one: each write is a layout change.
    pub fn set_state(&self, id: InstanceId, state: String) {
        if self.state(id) != state {
            self.edit(|d| {
                if let Some(p) = d.panel_mut(id) {
                    p.state = state;
                }
            });
        }
    }

    /// Opens a new `kind` panel where the kind lives.
    pub fn open(&self, kind: &str) -> Option<InstanceId> {
        let k = *self.catalog.read().get(kind)?;
        let id = self.edit(|d| d.add(Panel::new(k.kind), k.home).ok())?;
        if k.chrome == Chrome::Bare {
            self.edit(|d| set_chrome(d, id, Chrome::Bare));
        }
        Some(id)
    }

    fn title(&self, panel: &Panel) -> String {
        panel.title.clone().unwrap_or_else(|| {
            self.catalog
                .read()
                .get(panel.kind.as_str())
                .map_or_else(|| panel.kind.as_str().to_owned(), |k| k.title.to_owned())
        })
    }

    fn short(&self, panel: &Panel) -> String {
        self.catalog.read().get(panel.kind.as_str()).map_or_else(
            || panel.kind.as_str().chars().take(3).collect(),
            |k| k.short.to_owned(),
        )
    }

    fn mount(&self, at: Mount, data: Rc<MountedData>) {
        let mut mounts = self.mounts;
        mounts.write().insert(at, data);
    }

    fn mounted(&self, at: Mount) -> Option<Rc<MountedData>> {
        self.mounts.read().get(&at).cloned()
    }

    /// Measures `at`, in window px.
    async fn rect(&self, at: Mount) -> Option<Rect> {
        let r = self.mounted(at)?.get_client_rect().await.ok()?;
        Some(Rect::new(
            r.origin.x as f32,
            r.origin.y as f32,
            r.size.width as f32,
            r.size.height as f32,
        ))
    }
}

/// Draws every stack holding only `id` with `chrome`: a bare kind opened
/// alone gets a stack of its own.
fn set_chrome(d: &mut Dockspace, id: InstanceId, chrome: Chrome) {
    let mark = |n: &mut Node| mark_bare(n, id, chrome);
    if let Some(n) = d.center.as_mut() {
        mark(n);
    }
    for side in Side::ALL {
        if let Some(n) = d.edges.get_mut(side).root.as_mut() {
            mark(n);
        }
    }
    for f in &mut d.floats {
        mark(&mut f.root);
    }
}

fn mark_bare(n: &mut Node, id: InstanceId, chrome: Chrome) {
    match n {
        Node::Stack(s) if s.tabs == [id] => s.chrome = chrome,
        Node::Stack(_) => {}
        Node::Split { first, second, .. } => {
            mark_bare(first, id, chrome);
            mark_bare(second, id, chrome);
        }
    }
}

fn point(e: &PointerEvent) -> (f32, f32) {
    let p = e.data().client_coordinates();
    (p.x as f32, p.y as f32)
}

/// The whole dock: edges, centre, floats, and the overlays over them.
#[component]
pub fn DockHost() -> Element {
    let dock = use_dock();
    let space = dock.space();
    let class = (dock.class)();
    let body = match space.maximized.and_then(|id| space.panel(id).map(|p| p.id)) {
        Some(id) => rsx! { Maximized { id } },
        None if class.is_phone() => {
            rsx! { PhoneBody { space: space.clone(), portrait: class == FormClass::PhonePortrait } }
        }
        None => rsx! { DeskBody { space: space.clone() } },
    };
    rsx! {
        div {
            style: "position:relative; width:100%; height:100%; min-width:0; min-height:0; \
                    display:flex; overflow:hidden; background:{tokens::DESK}; color:{tokens::INK}; \
                    font-size:13px;",
            tabindex: "0",
            onmounted: move |e| dock.mount(Mount::Host, e.data()),
            onkeydown: move |e| {
                let key = *dock.key.read();
                if e.code() == key.code && (!key.shift || e.modifiers().shift()) {
                    dock.set_arranging(true);
                }
            },
            onkeyup: move |e| {
                let key = *dock.key.read();
                if e.code() == key.code || (key.shift && e.key() == Key::Shift) {
                    dock.set_arranging(false);
                }
            },
            onblur: move |_| dock.set_arranging(false),
            {body}
            DragLayer {}
            MenuLayer {}
            SheetLayer {}
        }
    }
}

/// Edges around the centre, floats over them.
#[component]
fn DeskBody(space: Dockspace) -> Element {
    let e = &space.edges;
    let center = rsx! {
        div { style: "flex:1 1 0; min-width:0; min-height:0; display:flex; position:relative;",
            match space.center.clone() {
                Some(node) => rsx! { NodeView { node } },
                None => rsx! { EmptyCenter {} },
            }
        }
    };
    let edge = |side: Side| {
        let edge = e.get(side).clone();
        rsx! { EdgeView { side, edge } }
    };
    let (outer, inner) = match e.corners {
        Corners::Sides => ("row", "column"),
        Corners::Ends => ("column", "row"),
    };
    let (a, b, c, d) = match e.corners {
        Corners::Sides => (Side::Left, Side::Right, Side::Top, Side::Bottom),
        Corners::Ends => (Side::Top, Side::Bottom, Side::Left, Side::Right),
    };
    rsx! {
        div { style: "flex:1 1 0; min-width:0; min-height:0; display:flex; flex-direction:{outer};",
            {edge(a)}
            div { style: "flex:1 1 0; min-width:0; min-height:0; display:flex; flex-direction:{inner};",
                {edge(c)}
                {center}
                {edge(d)}
            }
            {edge(b)}
        }
        for float in space.floats.iter().cloned() {
            FloatView { key: "{float.id}", float }
        }
    }
}

#[component]
fn EmptyCenter() -> Element {
    let mut dock = use_dock();
    rsx! {
        div {
            style: "flex:1; display:flex; align-items:center; justify-content:center; \
                    flex-direction:column; gap:12px; color:{tokens::INK_3};",
            div { "Nothing here yet" }
            button {
                style: "padding:10px 16px; border-radius:8px; border:1px solid {tokens::RULE}; \
                        background:{tokens::SHEET_2}; color:{tokens::INK}; min-height:44px;",
                onclick: move |_| dock.menu.set(Some(Menu::Add)),
                "Open a panel"
            }
        }
    }
}

/// One edge dock: open, a rail, or nothing.
#[component]
fn EdgeView(side: Side, edge: crate::dockspace::Edge) -> Element {
    let Some(root) = edge.root.clone() else {
        return rsx! {};
    };
    let across = side.axis() == Axis::Row;
    match edge.shown {
        Shown::Hidden => rsx! {},
        Shown::Collapsed => rsx! { Rail { side, root } },
        Shown::Open => {
            let size = if across {
                format!("width:{}px; min-width:0;", edge.size)
            } else {
                format!("height:{}px; min-height:0;", edge.size)
            };
            let divider = rsx! { EdgeDivider { side } };
            let body = rsx! {
                div { style: "flex:0 0 auto; {size} display:flex; position:relative; overflow:hidden;",
                    NodeView { node: root }
                }
            };
            if side.leads() {
                rsx! { {body} {divider} }
            } else {
                rsx! { {divider} {body} }
            }
        }
    }
}

/// A collapsed dock: its panels' short names; a press opens the dock.
#[component]
fn Rail(side: Side, root: Node) -> Element {
    let dock = use_dock();
    let space = dock.space();
    let across = side.axis() == Axis::Row;
    let shape = if across {
        format!("width:{RAIL}px; flex-direction:column;")
    } else {
        format!("height:{RAIL}px; flex-direction:row;")
    };
    let tabs: Vec<InstanceId> = root.stacks().iter().flat_map(|s| s.tabs.clone()).collect();
    rsx! {
        div {
            style: "flex:0 0 auto; {shape} display:flex; gap:4px; padding:4px; \
                    background:{tokens::SHEET}; border:0 solid {tokens::RULE}; overflow:hidden;",
            for id in tabs {
                if let Some(p) = space.panel(id) {
                    button {
                        key: "{id}",
                        style: "min-width:28px; min-height:28px; border-radius:6px; border:none; \
                                background:{tokens::SHEET_2}; color:{tokens::INK}; font-size:10px; \
                                font-weight:700;",
                        title: "{dock.title(p)}",
                        onclick: move |_| dock.edit(|d| {
                            d.set_shown(side, Shown::Open);
                            let _ = d.activate(id);
                        }),
                        "{dock.short(p)}"
                    }
                }
            }
        }
    }
}

/// The divider between an edge dock and the centre.
#[component]
fn EdgeDivider(side: Side) -> Element {
    let mut dock = use_dock();
    let cursor = if side.axis() == Axis::Row {
        "ew-resize"
    } else {
        "ns-resize"
    };
    let shape = if side.axis() == Axis::Row {
        format!("width:{DIVIDER}px;")
    } else {
        format!("height:{DIVIDER}px;")
    };
    rsx! {
        div {
            style: "flex:0 0 auto; {shape} background:{tokens::RULE}; cursor:{cursor}; touch-action:none;",
            onpointerdown: move |_| {
                spawn(async move {
                    if let Some(host) = dock.rect(Mount::Host).await {
                        dock.drag.set(Some(Drag::Edge { side, host }));
                    }
                });
            },
            ondoubleclick: move |_| dock.edit(|d| d.set_shown(side, Shown::Collapsed)),
        }
    }
}

/// A split or a stack.
#[component]
fn NodeView(node: Node) -> Element {
    match node {
        Node::Stack(stack) => rsx! { StackView { key: "{stack.id}", stack } },
        Node::Split {
            id,
            axis,
            share,
            first,
            second,
        } => rsx! {
            SplitView { id, axis, share, first: *first, second: *second }
        },
    }
}

#[component]
fn SplitView(id: SplitId, axis: Axis, share: Share, first: Node, second: Node) -> Element {
    let mut dock = use_dock();
    let dir = if axis == Axis::Row { "row" } else { "column" };
    let (a, b) = match share {
        Share::Ratio(r) => (format!("flex:{r} 1 0;"), format!("flex:{} 1 0;", 1.0 - r)),
        Share::First(px) => (fixed(axis, px), "flex:1 1 0;".to_owned()),
        Share::Second(px) => ("flex:1 1 0;".to_owned(), fixed(axis, px)),
    };
    let cursor = if axis == Axis::Row {
        "ew-resize"
    } else {
        "ns-resize"
    };
    let bar = if axis == Axis::Row {
        format!("width:{DIVIDER}px;")
    } else {
        format!("height:{DIVIDER}px;")
    };
    rsx! {
        div {
            style: "flex:1 1 0; min-width:0; min-height:0; display:flex; flex-direction:{dir};",
            onmounted: move |e| dock.mount(Mount::Split(id), e.data()),
            div { style: "{a} min-width:0; min-height:0; display:flex; position:relative; overflow:hidden;",
                NodeView { node: first }
            }
            div {
                style: "flex:0 0 auto; {bar} background:{tokens::RULE}; cursor:{cursor}; touch-action:none;",
                onpointerdown: move |_| {
                    spawn(async move {
                        if let Some(r) = dock.rect(Mount::Split(id)).await {
                            let (origin, length) = match axis {
                                Axis::Row => (r.x, r.w),
                                Axis::Column => (r.y, r.h),
                            };
                            dock.drag.set(Some(Drag::Split { id, axis, share, origin, length }));
                        }
                    });
                },
                ondoubleclick: move |_| {
                    let reset = match share {
                        Share::Ratio(_) => Share::Ratio(0.5),
                        other => other,
                    };
                    let _ = dock.edit(|d| d.set_share(id, reset));
                },
            }
            div { style: "{b} min-width:0; min-height:0; display:flex; position:relative; overflow:hidden;",
                NodeView { node: second }
            }
        }
    }
}

fn fixed(axis: Axis, px: f32) -> String {
    match axis {
        Axis::Row => format!("flex:0 0 {px}px; width:{px}px;"),
        Axis::Column => format!("flex:0 0 {px}px; height:{px}px;"),
    }
}

/// A stack: its tabs (unless bare) over the panel on show.
#[component]
fn StackView(stack: Stack) -> Element {
    let dock = use_dock();
    let Some(active) = stack.active_tab() else {
        return rsx! {};
    };
    let id = stack.id;
    rsx! {
        div {
            style: "flex:1 1 0; min-width:0; min-height:0; display:flex; flex-direction:column; \
                    background:{tokens::SHEET};",
            onmounted: move |e| dock.mount(Mount::Stack(id), e.data()),
            if stack.chrome == Chrome::Tabs {
                TabStrip { tabs: stack.tabs.clone(), active }
            }
            div { style: "flex:1 1 0; min-width:0; min-height:0; display:flex; position:relative; overflow:hidden;",
                PanelSlot { key: "{active}", id: active }
                if dock.arranging() {
                    Grip { tabs: stack.tabs.clone(), active, chrome: stack.chrome, stack: id }
                }
            }
        }
    }
}

/// Over a panel while arranging: its name (and its siblings', to switch
/// to), grabbed anywhere to drag it, right-clicked for its menu.
#[component]
fn Grip(tabs: Vec<InstanceId>, active: InstanceId, chrome: Chrome, stack: StackId) -> Element {
    let mut dock = use_dock();
    let space = dock.space();
    let mut grab = move |id: InstanceId, e: &PointerEvent| {
        let (x, y) = point(e);
        let _ = dock.edit(|d| d.activate(id));
        dock.drag.set(Some(Drag::Pressed { id, x, y }));
    };
    rsx! {
        div {
            style: "position:absolute; inset:0; z-index:5; display:flex; align-items:center; \
                    justify-content:center; background:{tokens::DROP_WASH}; \
                    border:2px solid {tokens::FOCUS}; cursor:grab; touch-action:none; user-select:none;",
            onpointerdown: move |e| grab(active, &e),
            oncontextmenu: move |e| {
                e.prevent_default();
                dock.drag.set(None);
                dock.menu.set(Some(Menu::Panel(active)));
            },
            div {
                style: "display:flex; flex-wrap:wrap; gap:6px; padding:8px; max-width:90%; \
                        justify-content:center; border-radius:10px; background:{tokens::SHEET_2};",
                for id in tabs {
                    if let Some(p) = space.panel(id) {
                        div {
                            key: "{id}",
                            style: "{grip_chip(id == active)}",
                            onpointerdown: move |e| {
                                e.stop_propagation();
                                grab(id, &e);
                            },
                            "{dock.title(p)}"
                        }
                    }
                }
                div {
                    style: "{grip_chip(false)}",
                    title: "Show or hide the tab strip",
                    onpointerdown: move |e| {
                        e.stop_propagation();
                        let next = if chrome == Chrome::Tabs { Chrome::Bare } else { Chrome::Tabs };
                        let _ = dock.edit(|d| d.set_chrome(stack, next));
                    },
                    if chrome == Chrome::Tabs { "Hide tabs" } else { "Tabs" }
                }
            }
        }
    }
}

fn grip_chip(on: bool) -> String {
    format!(
        "min-height:32px; display:flex; align-items:center; padding:0 12px; border-radius:8px; \
         white-space:nowrap; font-weight:600; color:{}; background:{}; border:1px solid {};",
        tokens::INK,
        if on { tokens::PRIMARY } else { tokens::SHEET },
        if on { tokens::PRIMARY } else { tokens::RULE },
    )
}

#[component]
fn TabStrip(tabs: Vec<InstanceId>, active: InstanceId) -> Element {
    let mut dock = use_dock();
    let space = dock.space();
    rsx! {
        div {
            style: "flex:0 0 {TABS_H}px; height:{TABS_H}px; display:flex; align-items:stretch; \
                    border-bottom:1px solid {tokens::RULE}; overflow:hidden; min-width:0;",
            div { style: "flex:1 1 0; min-width:0; display:flex; overflow:hidden;",
                for id in tabs {
                    if let Some(p) = space.panel(id) {
                        div {
                            key: "{id}",
                            style: tab_style(id == active),
                            onpointerdown: move |e| {
                                let (x, y) = point(&e);
                                let _ = dock.edit(|d| d.activate(id));
                                dock.drag.set(Some(Drag::Pressed { id, x, y }));
                            },
                            oncontextmenu: move |e| {
                                e.prevent_default();
                                dock.drag.set(None);
                                dock.menu.set(Some(Menu::Panel(id)));
                            },
                            ondoubleclick: move |_| {
                                dock.drag.set(None);
                                let _ = dock.edit(|d| d.toggle_maximized(id));
                            },
                            "{dock.title(p)}"
                        }
                    }
                }
            }
            button {
                style: "flex:0 0 auto; width:36px; border:none; background:transparent; \
                        color:{tokens::INK_3}; font-size:16px;",
                title: "Panel menu",
                onclick: move |_| dock.menu.set(Some(Menu::Panel(active))),
                "⋯"
            }
        }
    }
}

fn tab_style(on: bool) -> String {
    format!(
        "flex:0 1 auto; min-width:0; display:flex; align-items:center; padding:0 12px; \
         white-space:nowrap; overflow:hidden; touch-action:none; user-select:none; \
         color:{}; background:{}; border-right:1px solid {};",
        if on { tokens::INK } else { tokens::INK_3 },
        if on { tokens::SHEET_2 } else { "transparent" },
        tokens::RULE,
    )
}

/// One panel instance, drawn by its kind.
#[component]
fn PanelSlot(id: InstanceId) -> Element {
    let dock = use_dock();
    use_context_provider(|| ThisPanel(id));
    let kind = dock.space().panel(id).map(|p| p.kind.clone());
    let found = kind
        .as_ref()
        .and_then(|k| dock.catalog.read().get(k.as_str()).map(|k| k.render));
    match found {
        Some(render) => render(id),
        None => {
            let name = kind.map(|k| k.0).unwrap_or_default();
            rsx! {
                div { style: "flex:1; display:flex; align-items:center; justify-content:center; color:{tokens::INK_3};",
                    "No panel called “{name}” in this app"
                }
            }
        }
    }
}

/// A floating window: a title bar to drag, its own tree, a resize corner.
#[component]
fn FloatView(float: crate::dockspace::Float) -> Element {
    let mut dock = use_dock();
    let b = float.bounds;
    let id = float.id;
    let first = float.root.stacks().first().and_then(|s| s.active_tab());
    let space = dock.space();
    let title = first
        .and_then(|p| space.panel(p))
        .map(|p| dock.title(p))
        .unwrap_or_default();
    rsx! {
        div {
            style: "position:absolute; left:{b.x}px; top:{b.y}px; width:{b.w}px; height:{b.h}px; \
                    display:flex; flex-direction:column; background:{tokens::SHEET}; \
                    border:1px solid {tokens::RULE}; border-radius:8px; overflow:hidden; \
                    box-shadow:0 12px 32px rgba(0,0,0,0.5); z-index:10;",
            div {
                style: "flex:0 0 24px; display:flex; align-items:center; padding:0 8px; \
                        background:{tokens::SHEET_2}; color:{tokens::INK_3}; cursor:grab; \
                        touch-action:none; user-select:none; font-size:11px;",
                onpointerdown: move |e| {
                    let (x, y) = point(&e);
                    spawn(async move {
                        if let Some(host) = dock.rect(Mount::Host).await {
                            dock.drag.set(Some(Drag::Float { id, host, dx: x - host.x - b.x, dy: y - host.y - b.y }));
                        }
                    });
                },
                div { style: "flex:1; min-width:0; overflow:hidden; white-space:nowrap;", "{title}" }
                if let Some(p) = first {
                    button {
                        style: "border:none; background:transparent; color:{tokens::INK_3}; min-width:24px;",
                        title: "Dock it",
                        onclick: move |_| { let _ = dock.edit(|d| d.move_panel(p, Place::Center)); },
                        "⇲"
                    }
                }
            }
            div { style: "flex:1 1 0; min-height:0; display:flex; position:relative;",
                NodeView { node: float.root.clone() }
            }
            div {
                style: "position:absolute; right:0; bottom:0; width:16px; height:16px; cursor:nwse-resize; touch-action:none;",
                onpointerdown: move |_| {
                    spawn(async move {
                        if let Some(host) = dock.rect(Mount::Host).await {
                            dock.drag.set(Some(Drag::FloatSize { id, host }));
                        }
                    });
                },
            }
        }
    }
}

/// One panel filling the window, with a way back.
#[component]
fn Maximized(id: InstanceId) -> Element {
    let dock = use_dock();
    let space = dock.space();
    let title = space.panel(id).map(|p| dock.title(p)).unwrap_or_default();
    rsx! {
        div { style: "flex:1; min-width:0; min-height:0; display:flex; flex-direction:column;",
            div {
                style: "flex:0 0 {TABS_H}px; display:flex; align-items:center; gap:8px; padding:0 8px; \
                        border-bottom:1px solid {tokens::RULE}; background:{tokens::SHEET_2};",
                div { style: "flex:1; min-width:0; overflow:hidden; white-space:nowrap;", "{title}" }
                button {
                    style: "border:none; background:transparent; color:{tokens::INK}; min-height:28px;",
                    onclick: move |_| { let _ = dock.edit(|d| d.toggle_maximized(id)); },
                    "Restore"
                }
            }
            div { style: "flex:1 1 0; min-height:0; display:flex; position:relative;",
                PanelSlot { key: "{id}", id }
            }
        }
    }
}

/// On a phone: the centre alone, each docked panel a button opening it as
/// a sheet.
#[component]
fn PhoneBody(space: Dockspace, portrait: bool) -> Element {
    let mut dock = use_dock();
    let docked: Vec<InstanceId> = space
        .areas()
        .filter(|(area, _)| *area != AreaRef::Center)
        .flat_map(|(_, root)| {
            root.stacks()
                .into_iter()
                .flat_map(|s| s.tabs.clone())
                .collect::<Vec<_>>()
        })
        .collect();
    let arrange_bg = if (dock.latched)() {
        tokens::PRIMARY
    } else {
        "transparent"
    };
    let center = rsx! {
        div { style: "flex:1 1 0; min-width:0; min-height:0; display:flex; position:relative;",
            match space.center.clone() {
                Some(node) => rsx! { NodeView { node } },
                None => rsx! { EmptyCenter {} },
            }
        }
    };
    let (dir, bar) = if portrait {
        (
            "column",
            format!(
                "flex:0 0 {PHONE_BAR}px; flex-direction:row; border-top:1px solid {};",
                tokens::RULE
            ),
        )
    } else {
        (
            "row-reverse",
            format!(
                "flex:0 0 {PHONE_BAR}px; flex-direction:column; border-right:1px solid {};",
                tokens::RULE
            ),
        )
    };
    rsx! {
        div { style: "flex:1; min-width:0; min-height:0; display:flex; flex-direction:{dir};",
            {center}
            div {
                style: "{bar} display:flex; align-items:center; justify-content:space-around; \
                        gap:4px; padding:4px; background:{tokens::SHEET}; overflow:hidden;",
                for id in docked {
                    if let Some(p) = space.panel(id) {
                        button {
                            key: "{id}",
                            style: "min-width:48px; min-height:48px; border-radius:10px; border:none; \
                                    background:{tokens::SHEET_2}; color:{tokens::INK}; font-size:11px; font-weight:700;",
                            title: "{dock.title(p)}",
                            onclick: move |_| dock.sheet.set(Some(id)),
                            "{dock.short(p)}"
                        }
                    }
                }
                button {
                    style: "min-width:48px; min-height:48px; border-radius:10px; border:none; \
                            background:{arrange_bg}; color:{tokens::INK}; font-size:18px;",
                    title: "Move panels",
                    onclick: move |_| dock.toggle_arrange(),
                    "✥"
                }
                button {
                    style: "min-width:48px; min-height:48px; border-radius:10px; border:none; \
                            background:transparent; color:{tokens::INK_3}; font-size:20px;",
                    title: "Open a panel",
                    onclick: move |_| dock.menu.set(Some(Menu::Add)),
                    "+"
                }
            }
        }
    }
}

/// A phone's docked panel, opened over the centre.
#[component]
fn SheetLayer() -> Element {
    let mut dock = use_dock();
    let Some(id) = (dock.sheet)() else {
        return rsx! {};
    };
    let space = dock.space();
    let Some(panel) = space.panel(id) else {
        return rsx! {};
    };
    let portrait = (dock.class)() == FormClass::PhonePortrait;
    let card = if portrait {
        "left:0; right:0; bottom:0; height:78%; border-radius:14px 14px 0 0;"
    } else {
        "left:0; top:0; bottom:0; width:72%; border-radius:0 14px 14px 0;"
    };
    rsx! {
        div {
            style: "position:absolute; inset:0; background:{tokens::SCRIM}; z-index:30;",
            onclick: move |_| dock.sheet.set(None),
        }
        div {
            style: "position:absolute; {card} z-index:31; display:flex; flex-direction:column; \
                    background:{tokens::SHEET}; overflow:hidden;",
            div {
                style: "flex:0 0 48px; display:flex; align-items:center; padding:0 12px; \
                        border-bottom:1px solid {tokens::RULE};",
                div { style: "flex:1; font-weight:600;", "{dock.title(panel)}" }
                button {
                    style: "min-width:48px; min-height:48px; border:none; background:transparent; color:{tokens::INK}; font-size:18px;",
                    onclick: move |_| dock.sheet.set(None),
                    "✕"
                }
            }
            div { style: "flex:1 1 0; min-height:0; display:flex; position:relative;",
                PanelSlot { key: "{id}", id }
            }
        }
    }
}

/// While anything drags: a layer over everything that follows the pointer
/// and shows where a panel would land.
#[component]
fn DragLayer() -> Element {
    let mut dock = use_dock();
    let Some(drag) = (dock.drag)() else {
        return rsx! {};
    };
    let highlight = match &drag {
        Drag::Panel {
            at: Some(t),
            geometry: Some((host, _)),
            ..
        } => Some(Rect::new(
            t.highlight.x - host.x,
            t.highlight.y - host.y,
            t.highlight.w,
            t.highlight.h,
        )),
        _ => None,
    };
    let cursor = if matches!(drag, Drag::Panel { .. }) {
        "grabbing"
    } else {
        "default"
    };
    rsx! {
        div {
            style: "position:absolute; inset:0; z-index:40; touch-action:none; \
                    cursor:{cursor};",
            onpointermove: move |e| on_move(dock, point(&e)),
            onpointerup: move |_| on_up(dock),
            onpointerleave: move |_| dock.drag.set(None),
            if let Some(h) = highlight {
                div {
                    style: "position:absolute; left:{h.x}px; top:{h.y}px; width:{h.w}px; height:{h.h}px; \
                            background:{tokens::DROP_WASH}; border:2px solid {tokens::FOCUS}; \
                            border-radius:4px; pointer-events:none;",
                }
            }
        }
    }
}

fn on_move(mut dock: Dock, (x, y): (f32, f32)) {
    let Some(drag) = dock.drag.peek().clone() else {
        return;
    };
    match drag {
        Drag::Pressed { id, x: x0, y: y0 } => {
            if (x - x0).hypot(y - y0) > DRAG_SLOP {
                dock.drag.set(Some(Drag::Panel {
                    id,
                    geometry: None,
                    at: None,
                }));
                spawn(async move { measure_for_drop(dock, id).await });
            }
        }
        Drag::Panel {
            id,
            geometry: Some((host, stacks)),
            ..
        } => {
            let at = hit::target(x, y, host, &stacks);
            dock.drag.set(Some(Drag::Panel {
                id,
                geometry: Some((host, stacks)),
                at,
            }));
        }
        Drag::Panel { .. } => {}
        Drag::Split {
            id,
            axis,
            share,
            origin,
            length,
        } => {
            let first = ((match axis {
                Axis::Row => x,
                Axis::Column => y,
            }) - origin)
                .clamp(0.0, length);
            let next = match share {
                Share::Ratio(_) if length > 0.0 => Share::Ratio(first / length),
                Share::Ratio(r) => Share::Ratio(r),
                Share::First(_) => Share::First(first),
                Share::Second(_) => Share::Second(length - first - DIVIDER),
            };
            let _ = dock.edit(|d| d.set_share(id, next));
        }
        Drag::Edge { side, host } => {
            let size = match side {
                Side::Left => x - host.x,
                Side::Right => host.x + host.w - x,
                Side::Top => y - host.y,
                Side::Bottom => host.y + host.h - y,
            };
            let limit = if side.axis() == Axis::Row {
                host.w
            } else {
                host.h
            } * 0.8;
            dock.edit(|d| d.set_edge_size(side, size.clamp(RAIL, limit)));
        }
        Drag::Float { id, host, dx, dy } => {
            let _ = dock.edit(|d| {
                let Some(b) = d.floats.iter().find(|f| f.id == id).map(|f| f.bounds) else {
                    return;
                };
                let nx = (x - host.x - dx).clamp(0.0, (host.w - 48.0).max(0.0));
                let ny = (y - host.y - dy).clamp(0.0, (host.h - 24.0).max(0.0));
                let _ = d.set_float_bounds(id, Rect::new(nx, ny, b.w, b.h));
            });
        }
        Drag::FloatSize { id, host } => {
            let _ = dock.edit(|d| {
                let Some(b) = d.floats.iter().find(|f| f.id == id).map(|f| f.bounds) else {
                    return;
                };
                let w = (x - host.x - b.x).max(160.0);
                let h = (y - host.y - b.y).max(96.0);
                let _ = d.set_float_bounds(id, Rect::new(b.x, b.y, w, h));
            });
        }
    }
}

async fn measure_for_drop(mut dock: Dock, id: InstanceId) {
    let Some(host) = dock.rect(Mount::Host).await else {
        dock.drag.set(None);
        return;
    };
    let stack_ids: Vec<StackId> = dock
        .space()
        .areas()
        .flat_map(|(_, root)| root.stacks().into_iter().map(|s| s.id).collect::<Vec<_>>())
        .collect();
    let mut stacks = Vec::with_capacity(stack_ids.len());
    for s in stack_ids {
        if let Some(r) = dock.rect(Mount::Stack(s)).await {
            stacks.push((s, r));
        }
    }
    // Floats sit over the docks: test them first.
    let floating: Vec<StackId> = dock
        .space()
        .floats
        .iter()
        .flat_map(|f| {
            f.root
                .stacks()
                .into_iter()
                .map(|s| s.id)
                .collect::<Vec<_>>()
        })
        .collect();
    stacks.sort_by_key(|(s, _)| !floating.contains(s));
    if matches!(dock.drag.peek().as_ref(), Some(Drag::Panel { id: d, .. }) if *d == id) {
        dock.drag.set(Some(Drag::Panel {
            id,
            geometry: Some((host, stacks)),
            at: None,
        }));
    }
}

fn on_up(mut dock: Dock) {
    let drag = dock.drag.peek().clone();
    dock.drag.set(None);
    if let Some(Drag::Panel {
        id, at: Some(t), ..
    }) = drag
    {
        let _ = dock.edit(|d| d.move_panel(id, t.place));
    }
}

/// The panel menu and the open-a-panel menu.
#[component]
fn MenuLayer() -> Element {
    let mut dock = use_dock();
    let Some(menu) = (dock.menu)() else {
        return rsx! {};
    };
    let items = match menu {
        Menu::Panel(id) => panel_items(dock, id),
        Menu::Add => add_items(dock),
    };
    rsx! {
        div {
            style: "position:absolute; inset:0; z-index:50; background:{tokens::SCRIM}; \
                    display:flex; align-items:center; justify-content:center;",
            onclick: move |_| dock.menu.set(None),
            div {
                style: "min-width:240px; max-width:90%; max-height:85%; overflow-y:scroll; \
                        display:flex; flex-direction:column; padding:6px; border-radius:12px; \
                        background:{tokens::SHEET_2}; border:1px solid {tokens::RULE};",
                onclick: move |e| e.stop_propagation(),
                for (i, item) in items.into_iter().enumerate() {
                    MenuRow { key: "{i}", item }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
enum Item {
    Heading(String),
    Act(String, Action),
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Move(InstanceId, Place),
    Attach(InstanceId, InstanceId, Side, f32),
    Duplicate(InstanceId),
    Maximize(InstanceId),
    Collapse(Side),
    Close(InstanceId),
    Open(&'static str),
    Recall(ScreensetId),
    Show(InstanceId),
    Chrome(StackId, Chrome),
}

#[component]
fn MenuRow(item: Item) -> Element {
    let mut dock = use_dock();
    match item {
        Item::Heading(text) => rsx! {
            div { style: "padding:10px 12px 4px; color:{tokens::INK_3}; font-size:11px; text-transform:uppercase;", "{text}" }
        },
        Item::Act(label, action) => rsx! {
            button {
                style: "display:flex; justify-content:flex-start; text-align:left; padding:0 12px; \
                        min-height:40px; border:none; border-radius:8px; background:transparent; \
                        color:{tokens::INK}; font-size:14px;",
                onclick: move |_| {
                    dock.menu.set(None);
                    run(dock, action);
                },
                "{label}"
            }
        },
    }
}

fn run(mut dock: Dock, action: Action) {
    match action {
        Action::Move(id, place) => {
            let _ = dock.edit(|d| d.move_panel(id, place));
        }
        Action::Attach(id, host, side, px) => {
            let _ = dock.edit(|d| d.attach(id, host, side, px));
        }
        Action::Duplicate(id) => {
            let _ = dock.edit(|d| {
                let panel = d.panel(id)?.clone();
                let (_, stack) = d.locate(id)?;
                let copy = Panel {
                    id: InstanceId::new(),
                    title: None,
                    ..panel
                };
                let place = Place::Stack(
                    stack,
                    crate::dockspace::Zone::Split(Side::Right, Share::Ratio(0.5)),
                );
                d.add(copy, place).ok()
            });
        }
        Action::Maximize(id) => {
            let _ = dock.edit(|d| d.toggle_maximized(id));
        }
        Action::Collapse(side) => dock.edit(|d| d.set_shown(side, Shown::Collapsed)),
        Action::Recall(id) => {
            dock.sheet.set(None);
            dock.recall(id);
        }
        Action::Show(id) => {
            let _ = dock.edit(|d| d.activate(id));
        }
        Action::Chrome(stack, chrome) => {
            let _ = dock.edit(|d| d.set_chrome(stack, chrome));
        }
        Action::Close(id) => {
            let _ = dock.edit(|d| d.close(id));
        }
        Action::Open(kind) => {
            if let Some(id) = dock.open(kind) {
                if (dock.class)().is_phone()
                    && dock
                        .space()
                        .locate(id)
                        .is_some_and(|(a, _)| a != AreaRef::Center)
                {
                    dock.sheet.set(Some(id));
                }
            }
        }
    }
}

fn panel_items(dock: Dock, id: InstanceId) -> Vec<Item> {
    let space = dock.space();
    let Some(panel) = space.panel(id) else {
        return Vec::new();
    };
    let mut items = vec![Item::Heading(dock.title(panel))];
    let float = Rect::new(80.0, 80.0, 640.0, 420.0);
    for (label, place) in [
        ("Move to the left", Place::Edge(Side::Left)),
        ("Move to the right", Place::Edge(Side::Right)),
        ("Move to the top", Place::Edge(Side::Top)),
        ("Move to the bottom", Place::Edge(Side::Bottom)),
        ("Move to the centre", Place::Center),
        ("Float", Place::Float(float)),
    ] {
        items.push(Item::Act(label.into(), Action::Move(id, place)));
    }
    if let Some((host_kind, side, px)) = dock
        .catalog
        .read()
        .get(panel.kind.as_str())
        .and_then(|k| k.attach)
    {
        for host in space.of_kind(host_kind) {
            let label = format!("Attach to {}", dock.title(host));
            items.push(Item::Act(label, Action::Attach(id, host.id, side, px)));
        }
    }
    if let Some(stack) = space.locate(id).and_then(|(_, s)| space.stack(s)) {
        if stack.chrome == Chrome::Bare {
            for other in stack.tabs.iter().filter(|t| **t != id) {
                if let Some(p) = space.panel(*other) {
                    items.push(Item::Act(
                        format!("Show {}", dock.title(p)),
                        Action::Show(*other),
                    ));
                }
            }
        }
        let (label, next) = match stack.chrome {
            Chrome::Tabs => ("Hide tabs", Chrome::Bare),
            Chrome::Bare => ("Show tabs", Chrome::Tabs),
        };
        items.push(Item::Act(label.into(), Action::Chrome(stack.id, next)));
    }
    items.push(Item::Act("Open another".into(), Action::Duplicate(id)));
    let max = if space.maximized == Some(id) {
        "Restore"
    } else {
        "Maximise"
    };
    items.push(Item::Act(max.into(), Action::Maximize(id)));
    if let Some((AreaRef::Edge(side), _)) = space.locate(id) {
        items.push(Item::Act(
            "Collapse this dock".into(),
            Action::Collapse(side),
        ));
    }
    items.push(Item::Act("Close".into(), Action::Close(id)));
    items
}

fn add_items(dock: Dock) -> Vec<Item> {
    let mut items = vec![Item::Heading("Screensets".into())];
    items.extend(dock.sets.read().sets.iter().map(|set| {
        let label = match set.slot {
            Some(n) => format!("{n}  {}", set.name),
            None => set.name.clone(),
        };
        Item::Act(label, Action::Recall(set.id))
    }));
    items.push(Item::Heading("Open a panel".into()));
    items.extend(
        dock.catalog
            .read()
            .kinds()
            .map(|k| Item::Act(k.title.into(), Action::Open(k.kind))),
    );
    items
}

/// The screensets as a row of buttons: recall, save, save as, add a panel.
#[component]
pub fn ScreensetBar() -> Element {
    let mut dock = use_dock();
    let class = (dock.class)();
    let sets = dock.sets.read();
    let active = sets.active;
    let dirty = sets.is_dirty(class);
    let rows: Vec<(ScreensetId, String)> = sets
        .sets
        .iter()
        .map(|s| {
            let label = match s.slot {
                Some(n) => format!("{n} {}", s.name),
                None => s.name.clone(),
            };
            (s.id, label)
        })
        .collect();
    let count = rows.len();
    drop(sets);
    let chip = |on: bool| {
        format!(
            "flex:0 0 auto; min-height:32px; padding:0 12px; border-radius:8px; white-space:nowrap; \
             border:1px solid {}; background:{}; color:{};",
            if on { tokens::FOCUS } else { tokens::RULE },
            if on { tokens::SHEET_2 } else { "transparent" },
            tokens::INK,
        )
    };
    rsx! {
        div { style: "display:flex; align-items:center; gap:6px; min-width:0; overflow:hidden;",
            div { style: "display:flex; gap:6px; min-width:0; overflow-x:scroll; scrollbar-width:none;",
                for (id, label) in rows {
                    button {
                        key: "{id:?}",
                        style: "{chip(active == Some(id))}",
                        onclick: move |_| dock.recall(id),
                        if active == Some(id) && dirty { "{label} •" } else { "{label}" }
                    }
                }
            }
            button {
                style: "{chip(false)}",
                disabled: !dirty,
                onclick: move |_| { dock.sets.write().save(class); },
                "Save"
            }
            button {
                style: "{chip(false)}",
                onclick: move |_| { dock.sets.write().save_as(format!("Screenset {}", count + 1), class); },
                "Save as"
            }
            button {
                style: "{chip((dock.latched)())}",
                title: "Move panels: or hold W",
                onclick: move |_| dock.toggle_arrange(),
                "Arrange"
            }
            button {
                style: "{chip(false)}",
                onclick: move |_| dock.menu.set(Some(Menu::Add)),
                "+ Panel"
            }
        }
    }
}
