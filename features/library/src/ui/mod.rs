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
mod home;
mod kit;
mod lists;
mod project;
mod setlist;
mod song;

use dioxus::prelude::*;
use lucide_dioxus::{
    AudioWaveform, ChevronLeft, FolderKanban, Guitar, House, ListMusic, Music, Search, Users,
};

use crate::model::{
    AlbumId, ArrangementId, EntryId, Library, PartId, ProjectId, SessionId, SetlistId, SongId,
    SpaceId,
};
pub use kit::SHEET_CSS;
use kit::*;

/// The library a screen reads. Provided by the app (the synced docs);
/// the fixture when it is not.
#[derive(Clone, Copy)]
pub struct LibraryData(pub Signal<Library>);

/// The window's size in points, kept current by the app (a rotation
/// changes it).
#[derive(Clone, Copy)]
pub struct Viewport(pub Signal<(f32, f32)>);

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

/// The page the library opens on, when not Home (a deep link, a
/// screenshot of one page).
#[derive(Clone, Copy)]
pub struct InitialRoute(pub Route);

/// The app's handler for [`Open`].
#[derive(Clone, Copy)]
pub struct Opener(pub Callback<Open>);

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
}

impl Nav {
    /// Go to a page, keeping the way back.
    pub fn go(mut self, to: Route) {
        let here = *self.route.peek();
        if here != to {
            self.back.write().push(here);
            self.route.set(to);
        }
    }

    /// Go to a sidebar destination: a fresh start, no way back.
    pub fn jump(mut self, to: Route) {
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

/// Whether the window is wide enough for a sidebar beside the page (an
/// iPad in landscape, a desk); upright it is a rail.
pub(crate) fn use_wide() -> bool {
    let Viewport(size) = use_context::<Viewport>();
    size().0 >= 1000.0
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

/// The library: the root a window mounts.
#[component]
pub fn LibraryApp() -> Element {
    if try_consume_context::<LibraryData>().is_none() {
        use_context_provider(|| LibraryData(Signal::new(crate::fixture::library())));
    }
    if try_consume_context::<Viewport>().is_none() {
        use_context_provider(|| Viewport(Signal::new((1194.0, 834.0))));
    }
    let first = try_consume_context::<InitialRoute>().map_or(Route::Home, |r| r.0);
    use_context_provider(|| Nav {
        route: Signal::new(first),
        back: Signal::new(Vec::new()),
        space: Signal::new(None),
    });
    let nav = use_nav();
    let route = (nav.route)();
    rsx! {
        div {
            style: "position: absolute; top: 0; left: 0; width: 100vw; height: 100vh; display: flex; flex-direction: column; overflow: hidden; \
                    background: {DESK}; color: {INK}; font-family: {FONT}; font-variant-numeric: tabular-nums;",
            document::Style { {SHEET_CSS} }
            AppBar {}
            div {
                style: "flex: 1; min-height: 0; display: flex;",
                Sidebar {}
                div {
                    style: "flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column;",
                    match route {
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
                        Route::Session(id) => rsx! { album::SessionPage { id } },
                    }
                }
            }
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
                    padding: 0 16px; background: {SHEET}; border-bottom: 1px solid {RULE_STRONG};",
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
            div {
                style: "flex: 1; max-width: 520px; height: 38px; margin: 0 auto; border-radius: 10px; \
                        background: {SHEET_2}; border: 1px solid {RULE}; display: flex; align-items: center; \
                        gap: 8px; padding: 0 12px; color: {INK_3};",
                Search { size: 16, color: INK_3 }
                span { style: "{META} {ONE_LINE}",
                    if wide { "Search projects, setlists, songs, sessions" } else { "Search" }
                }
            }
            span {
                style: "display: flex; align-items: center; gap: 6px; height: 32px; padding: 0 10px; border-radius: 16px; \
                        border: 1px solid {RULE_STRONG}; {META} white-space: nowrap;",
                if pending == 0 {
                    span { style: "width: 7px; height: 7px; border-radius: 4px; background: {LIVE};" }
                    span { style: "color: {INK_2};", "Up to date" }
                } else {
                    span { style: "width: 7px; height: 7px; border-radius: 4px; background: {WARN};" }
                    span { style: "color: {INK_2};", "Syncing {pending}" }
                }
            }
            Avatar { name: me, size: 32 }
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
