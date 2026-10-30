//! The library screens (feature `ui`): an app bar, the sidebar of what
//! there is (Home, Setlists, Songs, Albums, Sessions, Rigs, and the
//! spaces), and the page for the selection. iPad first: a sidebar beside
//! the page in landscape, a rail of icons upright, and touch targets of
//! 48 everywhere.
//!
//! The root takes no props (CLAUDE.md): the app provides the library
//! ([`LibraryData`]), the window's size ([`Viewport`]) and what a press
//! that leaves the library does ([`Opener`]) as context; whatever it does
//! not provide falls back to the fixture, an iPad landscape window, and
//! nothing.

mod album;
mod command;
mod home;
mod kit;
mod lists;
mod player;
mod project;
mod session;
mod setlist;
mod song;
mod talk;

use app_shell::ui::{Insets, Screen};
use dioxus::prelude::*;
use lucide_dioxus::{
    AudioWaveform, ChevronLeft, FolderKanban, Guitar, House, ListMusic, Music, Search, Users, X,
};

use crate::model::{
    AlbumId, ArrangementId, Edit, EntryId, Library, PartId, ProjectId, SessionId, SetlistId,
    SongId, SpaceId,
};
pub use command::{MenuKey, Menus, Overlay, use_menus};
pub use kit::SHEET_CSS;
use kit::*;
pub use player::{Player, Sheet as PlayerSheet};
pub use session::{SessionStart, Tab as SessionTab};

/// The library a screen reads. Provided by the app (the synced docs);
/// the fixture when it is not.
#[derive(Clone, Copy)]
pub struct LibraryData(pub Signal<Library>);

/// The window's size in points, kept current by the app (a rotation
/// changes it).
#[derive(Clone, Copy)]
pub struct Viewport(pub Signal<(f32, f32)>);

/// Where a person's edits go, when the app keeps the library somewhere
/// (the synced docs). Without it an edit changes the library in memory.
#[derive(Clone, Copy)]
pub struct Editor(pub Callback<Edit>);

/// Make an edit: through the app's [`Editor`] when it has one, else on
/// the library in memory.
pub(crate) fn edit(e: Edit) {
    if let Some(Editor(cb)) = try_consume_context::<Editor>() {
        cb.call(e);
    } else {
        let LibraryData(mut lib) = consume_context::<LibraryData>();
        // An edit naming what is no longer there changes nothing.
        let _ = lib.write().apply(e);
    }
}

/// Whether one of the library's text fields has the keyboard: a plain key
/// (`?`, a letter) is that field's, not the menus'. Read by an app that
/// hears the window's keys itself ([`Menus::key`]'s `typing`).
static FIELD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A text field took the keyboard (`true`) or let it go.
pub fn set_field_focused(on: bool) {
    FIELD.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Whether a text field has the keyboard ([`set_field_focused`]).
#[must_use]
pub fn field_focused() -> bool {
    FIELD.load(std::sync::atomic::Ordering::Relaxed)
}

/// What a press that leaves the library asks the app to do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Open {
    /// Play a setlist on a rig: Signal, with that part.
    Part(PartId),
    /// Rehearse a setlist with its charts and tracks: Session.
    Rehearse(SetlistId),
    /// Open a session: Session.
    Session(SessionId),
}

/// Room the app's own controls need at the bar's left (a Mac's window
/// buttons, drawn over the page), in points.
#[derive(Clone, Copy)]
pub struct BarInset(pub u32);

/// The page the library opens on, when not Home (a deep link, a
/// screenshot of one page).
#[derive(Clone, Copy)]
pub struct InitialRoute(pub Route);

/// A menu up as the library opens (a screenshot of one).
#[derive(Clone, Copy)]
pub struct InitialOverlay(pub Overlay);

/// The app's handler for [`Open`].
#[derive(Clone, Copy)]
pub struct Opener(pub Callback<Open>);

/// One of the app's commands, for ⌘K and `?` beside the library's own:
/// in Studio, an action of Session's Action List.
#[derive(Clone, PartialEq, Debug)]
pub struct AppCommand {
    /// What [`AppCommands::run`] runs it by.
    pub id: String,
    pub label: String,
    pub group: String,
    /// Its shortcut as the app says it (`"Shift+G"`); empty for none.
    pub keys: String,
    /// Whether it can run now.
    pub enabled: bool,
}

/// The app's commands: listed afresh each time a menu is drawn, and run.
#[derive(Clone, Copy)]
pub struct AppCommands {
    pub list: Callback<(), Vec<AppCommand>>,
    pub run: Callback<String>,
}

/// What brings the library back on screen when a command goes somewhere
/// in it from over another page (⌘K's Go to Setlists with a song open).
#[derive(Clone, Copy)]
pub struct ShowLibrary(pub Callback<()>);

/// What the app adds to the library's own screens: its controls at the
/// bar's right end in place of the avatar (the account), and a section at
/// the top of Home (its ways in: a new project, joining by a link, what
/// was opened lately). Each draws a component of the app's.
#[derive(Clone, Copy, Default)]
pub struct Slots {
    pub bar: Option<Callback<(), Element>>,
    pub home: Option<Callback<(), Element>>,
}

/// Whether a [`CommandLayer`] is over the library, and takes the menus'
/// keys and draws them.
#[derive(Clone, Copy)]
struct Layered;

/// Is a page's text field typing (when the layer is over a page that is
/// not the library)? A plain `?` is that field's then.
#[derive(Clone, Copy)]
pub struct Typing(pub Callback<(), bool>);

/// Where the library is.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Route {
    Home,
    Setlists,
    Projects,
    Songs,
    Sessions,
    Rigs,
    Project(ProjectId),
    Setlist(SetlistId, Option<EntryId>),
    Song(SongId, Option<ArrangementId>),
    Album(AlbumId),
    Session(SessionId),
}

impl Route {
    /// The sidebar destination a page belongs under.
    const fn section(self) -> Self {
        match self {
            Self::Setlist(..) => Self::Setlists,
            Self::Song(..) => Self::Songs,
            Self::Album(_) | Self::Project(_) => Self::Projects,
            Self::Session(_) => Self::Sessions,
            other => other,
        }
    }
}

/// Where the library is, where it has been, and which space it shows.
#[derive(Clone, Copy)]
pub struct Nav {
    pub route: Signal<Route>,
    back: Signal<Vec<Route>>,
    /// Only this space's docs; `None` is every space.
    pub space: Signal<Option<SpaceId>>,
    /// What the search field holds; while it holds anything, the page is
    /// its results.
    pub query: Signal<String>,
}

impl Nav {
    /// Go to a page, keeping the way back.
    pub fn go(mut self, to: Route) {
        self.query.set(String::new());
        let here = *self.route.peek();
        if here != to {
            self.back.write().push(here);
            self.route.set(to);
        }
    }

    /// Go to a sidebar destination: a fresh start, no way back.
    pub fn jump(mut self, to: Route) {
        self.query.set(String::new());
        self.back.write().clear();
        self.route.set(to);
    }

    pub fn can_go_back(self) -> bool {
        !self.back.read().is_empty()
    }

    pub fn go_back(mut self) {
        let prev = self.back.write().pop();
        if let Some(prev) = prev {
            self.route.set(prev);
        }
    }
}

pub(crate) fn use_library() -> Signal<Library> {
    use_context::<LibraryData>().0
}

pub(crate) fn use_nav() -> Nav {
    use_context::<Nav>()
}

pub(crate) fn open(what: Open) {
    if let Some(Opener(cb)) = try_consume_context::<Opener>() {
        cb.call(what);
    }
}

/// The window as the library lays out for it: the size the app hands it
/// ([`Viewport`]), classified by the apps' one rule
/// ([`app_shell::ui::FormFactor::of`]), with the touch and the insets of
/// the screen the host published (none on a desk, or with no host).
pub(crate) fn use_screen() -> Screen {
    let Viewport(size) = use_context::<Viewport>();
    let host = try_use_context::<Signal<Screen>>();
    let (w, h) = size();
    let (touch, insets) = host.map_or((false, Insets::ZERO), |s| {
        let s = s();
        (s.touch, s.insets)
    });
    Screen::of(f64::from(w), f64::from(h), touch, insets)
}

/// Whether the window is wide enough for a sidebar beside the page (an
/// iPad in landscape, a desk); upright it is a rail.
pub(crate) fn use_wide() -> bool {
    use_screen().width >= 1000.0
}

/// Whether the window is a phone's held upright: no sidebar, a tab bar at
/// the foot. A phone on its side keeps the rail (its height is the scarce
/// side).
pub(crate) fn use_phone() -> bool {
    let screen = use_screen();
    screen.phone() && !screen.landscape()
}

/// Whether the sidebar shows its labels and spaces, or folds to a rail.
/// A list has the room beside it on an iPad sideways; a setlist or a song
/// needs its table and its side panel, so there it folds below a desk's
/// width.
fn use_full_sidebar() -> bool {
    let Viewport(size) = use_context::<Viewport>();
    let detail = matches!(
        (use_nav().route)(),
        Route::Setlist(..)
            | Route::Song(..)
            | Route::Album(_)
            | Route::Session(_)
            | Route::Project(_)
    );
    let w = size().0;
    w >= 1366.0 || (w >= 1000.0 && !detail)
}

/// What the library reads that the app may not have provided, once, on
/// the first render (a hook, so every render calls the same hooks in the
/// same order): the library (the fixture), the window's size, where the
/// library is, and the keyboard menus. Kept when provided above.
fn use_library_contexts() -> (Nav, command::Keys) {
    use_hook(|| {
        if try_consume_context::<LibraryData>().is_none() {
            provide_context(LibraryData(Signal::new(crate::fixture::library())));
        }
        if try_consume_context::<Viewport>().is_none() {
            provide_context(Viewport(Signal::new((1194.0, 834.0))));
        }
    });
    let nav = use_hook(|| {
        try_consume_context::<Nav>().unwrap_or_else(|| {
            let first = try_consume_context::<InitialRoute>().map_or(Route::Home, |r| r.0);
            Nav {
                route: Signal::new(first),
                back: Signal::new(Vec::new()),
                space: Signal::new(None),
                query: Signal::new(String::new()),
            }
        })
    });
    use_context_provider(|| nav);
    let keys = use_hook(|| {
        try_consume_context::<command::Keys>().unwrap_or_else(|| {
            let keys = command::Keys::new();
            if let Some(InitialOverlay(up)) = try_consume_context::<InitialOverlay>() {
                keys.show(up);
            }
            keys
        })
    });
    use_context_provider(|| keys);
    (nav, keys)
}

/// The keyboard menus — ⌘K (the library's commands and the app's), ⌘O
/// (open by name) and `?` (every shortcut) — over whatever the app shows
/// under it: the library, or a song open in the DAW. Mount it at the
/// window's root, around every page, so the menus and where the library
/// is stay the same whichever page is up. `on_overlay` hears a menu go up
/// (`true`) and down: a page that reads the keyboard itself stands aside
/// while one is up.
#[component]
pub fn CommandLayer(
    on_overlay: Option<EventHandler<bool>>,
    /// The app feeds the menus the window's keys itself ([`Menus::key`]):
    /// a page's key reaches the DOM only through what has the focus, and
    /// at first nothing does.
    #[props(default)]
    window_keys: bool,
    children: Element,
) -> Element {
    let (_, keys) = use_library_contexts();
    use_context_provider(|| Layered);
    let menus = command::use_menus();
    let up = (keys.overlay)() != command::Overlay::None;
    let mut was = use_signal(|| false);
    if *was.peek() != up {
        was.set(up);
        if let Some(cb) = on_overlay {
            cb.call(up);
        }
    }
    rsx! {
        div {
            style: "position: absolute; top: 0; left: 0; width: 100vw; height: 100vh; overflow: hidden; \
                    color: {INK}; font-family: {FONT};",
            onkeydown: move |e| {
                // An app that hears the window's keys feeds them itself
                // (`Menus::key`); this is for one that does not.
                if !window_keys {
                    let typing = try_consume_context::<Typing>().is_some_and(|t| t.0.call(()));
                    command::on_menu_key(&e, menus, typing);
                }
            },
            document::Style { {SHEET_CSS} }
            {children}
            command::Overlays {}
        }
    }
}

/// The library: the root a window mounts.
#[component]
pub fn LibraryApp() -> Element {
    let (nav, keys) = use_library_contexts();
    let layered = try_use_context::<Layered>().is_some();
    let menus = command::use_menus();
    // The app's player when it provides one (a screenshot of it playing),
    // else the library's own.
    let player = use_hook(|| {
        try_consume_context::<player::Player>()
            .unwrap_or_else(|| provide_context(player::Player::new()))
    });
    player::use_clock(player);
    let phone = use_phone();
    let route = (nav.route)();
    rsx! {
        div {
            // The library takes the keys (command.rs): ⌘K, ⌘O, ?, ⌘1–6,
            // ⌘[, and what an open overlay is typed into.
            tabindex: "0",
            style: "position: absolute; top: 0; left: 0; width: 100vw; height: 100vh; display: flex; flex-direction: column; overflow: hidden; \
                    background: {DESK}; color: {INK}; font-family: {FONT}; font-variant-numeric: tabular-nums; outline: none;",
            onkeydown: move |e| {
                // Space plays and pauses, when no overlay is taking keys.
                if e.key() == Key::Character(" ".into())
                    && *keys.overlay.peek() == command::Overlay::None
                {
                    player.toggle();
                    e.prevent_default();
                    return;
                }
                // Under a layer, the menus' keys are the layer's (it hears
                // them as they bubble up from here).
                if layered {
                    if *keys.overlay.peek() == command::Overlay::None {
                        command::on_page_key(&e, nav);
                    }
                } else {
                    command::on_key(&e, menus);
                }
            },
            document::Style { {SHEET_CSS} }
            AppBar {}
            div {
                style: "flex: 1; min-height: 0; display: flex;",
                if !phone { Sidebar {} }
                div {
                    style: "flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column;",
                    match route {
                        _ if !(nav.query)().trim().is_empty() => rsx! { lists::Results {} },
                        Route::Home => rsx! { home::Home {} },
                        Route::Setlists => rsx! { lists::Setlists {} },
                        Route::Songs => rsx! { lists::Songs {} },
                        Route::Projects => rsx! { project::Projects {} },
                        Route::Sessions => rsx! { lists::Sessions {} },
                        Route::Rigs => rsx! { lists::Rigs {} },
                        Route::Setlist(id, entry) => rsx! { setlist::SetlistPage { id, entry } },
                        Route::Song(id, arrangement) => rsx! { song::SongPage { id, arrangement } },
                        Route::Project(id) => rsx! { project::ProjectPage { id } },
                        Route::Album(id) => rsx! { album::AlbumPage { id } },
                        Route::Session(id) => rsx! { session::SessionPage { id } },
                    }
                }
            }
            player::MiniPlayer {}
            if phone { TabBar {} }
            player::NowPlaying {}
            if !layered { command::Overlays {} }
        }
    }
}

/// The bar: back (when there is a way back), the search, how the library
/// stands against the account's other devices, and who is signed in.
#[component]
fn AppBar() -> Element {
    let nav = use_nav();
    let lib = use_library();
    let me = lib.read().me.clone();
    let wide = use_wide();
    let phone = use_phone();
    let inset = try_use_context::<BarInset>().map_or(16, |b| b.0);
    let pending = {
        let l = lib.read();
        l.songs
            .iter()
            .filter(|s| matches!(s.sync, crate::model::Sync::Syncing(_)))
            .count()
            + l.sessions
                .iter()
                .filter(|p| matches!(p.sync, crate::model::Sync::Syncing(_)))
                .count()
    };
    rsx! {
        div {
            style: "flex-shrink: 0; height: {BAR}px; display: flex; align-items: center; gap: 12px; \
                    padding: 0 16px 0 {inset}px; background: {SHEET}; border-bottom: 1px solid {RULE_STRONG};",
            if nav.can_go_back() {
                button {
                    class: "lp",
                    "aria-label": "Back",
                    style: "width: {HIT}px; height: 40px; border: none; border-radius: 10px; background: transparent; \
                            display: flex; align-items: center; justify-content: center; color: {INK};",
                    onclick: move |_| nav.go_back(),
                    ChevronLeft { size: 22, color: INK }
                }
            } else {
                span { style: "font-size: 20px; font-weight: 780; letter-spacing: -0.02em; padding: 0 6px;", "studio" }
            }
            SearchField { wide }
            if !phone { CommandButton {} }
            span {
                style: "display: flex; align-items: center; gap: 6px; height: 32px; padding: 0 10px; border-radius: 16px; \
                        border: 1px solid {RULE_STRONG}; {META} white-space: nowrap;",
                if pending == 0 {
                    span { style: "width: 7px; height: 7px; border-radius: 4px; background: {LIVE};" }
                    if !phone { span { style: "color: {INK_2};", "Up to date" } }
                } else {
                    span { style: "width: 7px; height: 7px; border-radius: 4px; background: {WARN};" }
                    if !phone { span { style: "color: {INK_2};", "Syncing {pending}" } }
                }
            }
            match try_use_context::<Slots>().and_then(|s| s.bar) {
                Some(bar) => bar.call(()),
                None => rsx! { Avatar { name: me, size: 32 } },
            }
        }
    }
}

/// ⌘K for a finger: the command palette, from a button.
#[component]
fn CommandButton() -> Element {
    let keys = use_context::<command::Keys>();
    rsx! {
        button {
            class: "lp",
            "aria-label": "Commands",
            style: "height: 36px; padding: 0 8px; border-radius: 10px; border: 1px solid {RULE_STRONG}; background: transparent; \
                    display: flex; align-items: center; gap: 4px;",
            onclick: move |_| keys.show(command::Overlay::Palette),
            command::Kbd { text: "⌘" }
            command::Kbd { text: "K" }
        }
    }
}

/// The search: what it holds replaces the page with its results; ✕ or
/// Escape clears it.
#[component]
fn SearchField(wide: bool) -> Element {
    let mut nav = use_nav();
    let q = (nav.query)();
    let empty = q.is_empty();
    rsx! {
        div {
            style: "flex: 1; max-width: 520px; height: 38px; margin: 0 auto; border-radius: 10px; \
                    background: {SHEET_2}; border: 1px solid {RULE}; display: flex; align-items: center; \
                    gap: 8px; padding: 0 6px 0 12px; color: {INK_3};",
            Search { size: 16, color: INK_3 }
            div { style: "position: relative; flex: 1; min-width: 0; height: 36px; display: flex; align-items: center;",
                if empty {
                    span { style: "position: absolute; left: 0; top: 9px; {META} {ONE_LINE} pointer-events: none;",
                        if wide { "Search projects, setlists, songs, sessions" } else { "Search" }
                    }
                }
                input {
                    onfocus: move |_| crate::ui::set_field_focused(true),
                    onblur: move |_| crate::ui::set_field_focused(false),
                    r#type: "text",
                    value: "{q}",
                    "aria-label": "Search",
                    style: "position: relative; width: 100%; height: 36px; border: none; outline: none; padding: 0; \
                            background: transparent; color: {INK}; font-family: {FONT}; font-size: 14px;",
                    oninput: move |e| nav.query.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Escape {
                            nav.query.set(String::new());
                        }
                        // A plain key is the field's; ⌘ ones go on to the
                        // library's shortcuts.
                        if !command::command_held(&e) {
                            e.stop_propagation();
                        }
                    },
                }
            }
            if !empty {
                button {
                    class: "lp",
                    "aria-label": "Clear search",
                    style: "width: 30px; height: 30px; border: none; border-radius: 8px; background: transparent; \
                            display: flex; align-items: center; justify-content: center;",
                    onclick: move |_| nav.query.set(String::new()),
                    X { size: 16, color: INK_2 }
                }
            }
        }
    }
}

/// What there is: the destinations, then the spaces to narrow them to.
#[component]
fn Sidebar() -> Element {
    let wide = use_full_sidebar();
    let lib = use_library();
    let nav = use_nav();
    let here = (nav.route)().section();
    let space = (nav.space)();
    let l = lib.read();
    let count = |n: usize| if wide { Some(n.to_string()) } else { None };
    let tonight = l.setlists.iter().any(|s| s.tonight);
    let width = if wide { 248 } else { 84 };
    let spaces = l.spaces.clone();
    rsx! {
        div {
            style: "width: {width}px; flex-shrink: 0; background: {SHEET}; border-right: 1px solid {RULE_STRONG}; \
                    display: flex; flex-direction: column; gap: 2px; padding: 12px 10px; overflow-y: scroll;",
            NavItem { to: Route::Home, here, label: "Home", count: None::<String>, dot: tonight,
                icon: rsx! { House { size: 20, color: if here == Route::Home { INK } else { INK_2 } } } }
            NavItem { to: Route::Projects, here, label: "Projects", count: count(l.projects.len()), dot: false,
                icon: rsx! { FolderKanban { size: 20, color: if here == Route::Projects { INK } else { INK_2 } } } }
            NavItem { to: Route::Setlists, here, label: "Setlists", count: count(l.setlists.len()), dot: false,
                icon: rsx! { ListMusic { size: 20, color: if here == Route::Setlists { INK } else { INK_2 } } } }
            NavItem { to: Route::Songs, here, label: "Songs", count: count(l.songs.len()), dot: false,
                icon: rsx! { Music { size: 20, color: if here == Route::Songs { INK } else { INK_2 } } } }
            NavItem { to: Route::Sessions, here, label: "Sessions", count: count(l.sessions.len()), dot: false,
                icon: rsx! { AudioWaveform { size: 20, color: if here == Route::Sessions { INK } else { INK_2 } } } }
            NavItem { to: Route::Rigs, here, label: "Rigs", count: count(l.rigs.iter().filter(|r| r.owner == l.me).count()), dot: false,
                icon: rsx! { Guitar { size: 20, color: if here == Route::Rigs { INK } else { INK_2 } } } }
            if wide {
                div { style: "height: 1px; background: {RULE}; margin: 14px 8px;" }
                span { style: "{COLUMN} color: {INK_3}; padding: 0 10px 6px;", "Spaces" }
                SpaceItem { id: None, label: "All spaces".to_owned(), band: false, members: 0, on: space.is_none() }
                for s in spaces {
                    SpaceItem { key: "{s.id.0}", id: Some(s.id), label: s.name.clone(), band: s.band, members: s.members.len(), on: space == Some(s.id) }
                }
            }
        }
    }
}

/// A phone's foot: four places, as a music app keeps them.
#[component]
fn TabBar() -> Element {
    let nav = use_nav();
    let here = (nav.route)().section();
    let tab = |to: Route| if here == to { INK } else { INK_3 };
    // Clear of the home indicator and (on its side) the housing, where the
    // page has to keep clear of them itself.
    let inset = use_screen().insets;
    let (left, right, bottom) = (8.0 + inset.left, 8.0 + inset.right, 8.0 + inset.bottom);
    let height = 56.0 + bottom;
    rsx! {
        div { style: "flex-shrink: 0; height: {height}px; box-sizing: border-box; display: flex; align-items: stretch; \
                      padding: 0 {right}px {bottom}px {left}px; \
                      background: {SHEET}; border-top: 1px solid {RULE_STRONG};",
            TabItem { to: Route::Home, label: "Home", colour: tab(Route::Home), icon: rsx! { House { size: 22, color: tab(Route::Home) } } }
            TabItem { to: Route::Projects, label: "Projects", colour: tab(Route::Projects), icon: rsx! { FolderKanban { size: 22, color: tab(Route::Projects) } } }
            TabItem { to: Route::Setlists, label: "Setlists", colour: tab(Route::Setlists), icon: rsx! { ListMusic { size: 22, color: tab(Route::Setlists) } } }
            TabItem { to: Route::Songs, label: "Songs", colour: tab(Route::Songs), icon: rsx! { Music { size: 22, color: tab(Route::Songs) } } }
        }
    }
}

#[component]
fn TabItem(to: Route, label: &'static str, colour: &'static str, icon: Element) -> Element {
    let nav = use_nav();
    rsx! {
        button { class: "lp", "aria-label": label,
            style: "flex: 1 1 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 3px; \
                    border: none; border-radius: 10px; background: transparent; color: {colour}; font-family: {FONT}; font-size: 11px; font-weight: 650;",
            onclick: move |_| nav.jump(to),
            {icon}
            span { "{label}" }
        }
    }
}

#[component]
fn NavItem(
    to: Route,
    here: Route,
    label: &'static str,
    count: Option<String>,
    dot: bool,
    icon: Element,
) -> Element {
    let nav = use_nav();
    let wide = use_full_sidebar();
    let on = to == here;
    let (bg, colour, weight) = if on {
        ("rgba(255,255,255,0.07)", INK, 700)
    } else {
        ("transparent", INK_2, 560)
    };
    let layout = if wide {
        "height: 44px; flex-direction: row; gap: 12px; padding: 0 12px;"
    } else {
        "height: 60px; flex-direction: column; justify-content: center; gap: 4px; padding: 0;"
    };
    let label_style = if wide {
        "font-size: 15px;"
    } else {
        "font-size: 11px;"
    };
    rsx! {
        button {
            class: "lp",
            "aria-label": label,
            style: "display: flex; align-items: center; {layout} border: none; border-radius: 10px; \
                    background: {bg}; color: {colour}; font-family: {FONT}; font-weight: {weight}; position: relative;",
            onclick: move |_| nav.jump(to),
            {icon}
            span { style: "{label_style} {ONE_LINE}", "{label}" }
            if wide {
                span { style: "flex: 1;" }
                if let Some(n) = count { span { style: "{META} color: {INK_3};", "{n}" } }
            }
            if dot {
                span { style: "position: absolute; top: 9px; left: 26px; width: 8px; height: 8px; border-radius: 4px; \
                               background: {LIVE}; border: 2px solid {SHEET};" }
            }
        }
    }
}

#[component]
fn SpaceItem(id: Option<SpaceId>, label: String, band: bool, members: usize, on: bool) -> Element {
    let mut nav = use_nav();
    let (bg, colour) = if on {
        ("rgba(255,255,255,0.07)", INK)
    } else {
        ("transparent", INK_2)
    };
    rsx! {
        button {
            class: "lp",
            style: "height: 40px; display: flex; align-items: center; gap: 10px; padding: 0 12px; border: none; \
                    border-radius: 10px; background: {bg}; color: {colour}; font-family: {FONT}; font-size: 14px; \
                    font-weight: 560;",
            onclick: move |_| nav.space.set(id),
            if band { Users { size: 17, color: INK_3 } } else { span { style: "width: 17px;" } }
            span { style: "{ONE_LINE}", "{label}" }
            span { style: "flex: 1;" }
            if band { span { style: "{META} color: {INK_3};", "{members}" } }
        }
    }
}
