//! Studio, remade from the launcher prototype: one Dioxus app for the
//! phone first, and later the desktop and the web. The content is made up
//! and in memory; on the web its sets are live on Drive (`live_sync`), and
//! its library and songs are Drive documents, each song a session
//! (`songs`, on `session_sync`); each org has its own library, team and
//! settings, and shares songs with others (`orgs`, `team`); its sets are
//! Drive documents too (`setlist` the model, `sets` live).

mod actions;
mod data;
#[cfg(feature = "drive")]
mod drive_sets;
#[cfg(feature = "native")]
mod engine;
// The session's views: the real extensions, hosted (natively).
mod files;
mod files_live;
#[cfg(feature = "native")]
mod hosted;
mod icons;
mod keys;
mod live_sync;
mod orgs;
mod pages;
mod pieces;
#[cfg(feature = "native")]
mod playback;
#[cfg(feature = "web")]
mod player;
mod storage;
#[cfg(feature = "drive")]
mod trace;
// The browser build engraves charts and reads setlist files from Drive.
#[cfg_attr(not(feature = "drive"), allow(dead_code))]
mod chart_sync;
#[cfg_attr(not(feature = "web"), allow(dead_code))]
mod reference;
mod session;
mod session_file;
// The session's Stage: its three windows' layout (natively).
#[cfg(feature = "drive")]
mod set_files;
mod set_sheets;
#[cfg_attr(not(feature = "drive"), allow(dead_code))]
mod setfile;
mod setlist;
mod sets;
mod sheets;
mod shell;
mod song_ids;
mod songs;
#[cfg(feature = "native")]
mod stage;
mod state;
mod team;

use dioxus::prelude::*;

use pages::drive_set::{DriveSet, NowChart};
use pages::home::Home;
use pages::library::{Library, LibraryHome};
use pages::live::{Chart, Lyrics};
use pages::orgs::{AllOrgs, OrgSettings, People};
use pages::print::PrintSet;
use pages::records::{Album, Records, TrackPage};
use pages::set::{OrgSet, SetPage};
use pages::setlists::{Every, Series, Setlists};
use pages::signal::Signal;
use pages::song::{SharedSong, SongPage};
use session::{SessionPage, SessionShell};
use shell::{Root, Shell};

pub(crate) const STYLE: Asset = asset!("/assets/style.css");
pub(crate) const APP_STYLE: Asset = asset!("/assets/app.css");
const FONT: &str = "https://fonts.googleapis.com/css2?family=Instrument+Sans:wdth,wght@75..100,400..700&display=swap";

/// Every page, as its place in the app, in its two modes: the launcher
/// (`Shell`: the four tabs are its roots, the rest pushed over them) and
/// the session (`SessionShell`: a set open for playing, the whole window).
/// Both under `Root`, which keeps playback and the Action List across them.
#[derive(Clone, Debug, PartialEq, Routable)]
#[rustfmt::skip]
pub enum Route {
    #[layout(Root)]
    #[layout(Shell)]
        #[route("/")]
        Home {},
        #[route("/records")]
        Records {},
        #[route("/records/:id")]
        Album { id: String },
        #[route("/track/:idx")]
        TrackPage { idx: usize },
        #[route("/setlists")]
        Setlists {},
        #[route("/setlists/every")]
        Every {},
        #[route("/series/:id")]
        Series { id: String },
        #[route("/set/:id")]
        SetPage { id: String },
        #[route("/set/:id/chart")]
        Chart { id: String },
        #[route("/set/:id/lyrics")]
        Lyrics { id: String },
        #[route("/drive/:space/:location/:..path")]
        DriveSet { space: String, location: String, path: Vec<String> },
        #[route("/now/chart")]
        NowChart {},

        #[route("/at/:owner/:id")]
        OrgSet { owner: String, id: String },
        #[route("/print/:owner/:id")]
        PrintSet { owner: String, id: String },
        #[route("/library")]
        LibraryHome {},
        #[route("/library/:tag")]
        Library { tag: String },
        #[route("/song/:id")]
        SongPage { id: String },
        #[route("/shared/:owner/:id")]
        SharedSong { owner: String, id: String },
        #[route("/signal")]
        Signal {},
        #[route("/orgs")]
        AllOrgs {},
        #[route("/people")]
        People {},
        #[route("/org")]
        OrgSettings {},
    #[end_layout]
    #[layout(SessionShell)]
        #[route("/session/:view")]
        SessionPage { view: String },
}

/// The four tabs at the foot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Home,
    Records,
    Setlists,
    Signal,
}

impl Route {
    /// The tab a page belongs to.
    pub fn tab(&self) -> Tab {
        match self {
            Route::Home {} => Tab::Home,
            Route::Records {} | Route::Album { .. } | Route::TrackPage { .. } => Tab::Records,
            Route::Signal {} => Tab::Signal,
            Route::AllOrgs {} | Route::People {} | Route::OrgSettings {} => Tab::Home,
            _ => Tab::Setlists,
        }
    }

    /// The set a page shows, if it is one of a set's views.
    pub fn set_id(&self) -> Option<&str> {
        match self {
            Route::SetPage { id }
            | Route::Chart { id }
            | Route::Lyrics { id }
            | Route::OrgSet { id, .. } => Some(id),
            _ => None,
        }
    }

    /// Where to open a set or a show, by its id.
    pub fn set(id: &str) -> Route {
        Route::SetPage { id: id.to_string() }
    }

    /// Where an id from the prototype's data opens: a record, a track, a
    /// recurring event, a set (a built-in show in its own org).
    pub fn open(id: &str) -> Route {
        if data::album(id).is_some() {
            Route::Album { id: id.into() }
        } else if let Some(t) = id.strip_prefix("t:").and_then(data::track) {
            Route::TrackPage { idx: t.idx }
        } else if data::series(id).is_some() {
            Route::Series { id: id.into() }
        } else if let Some(owner) = setlist::builtin_home(id) {
            Route::OrgSet {
                owner: owner.into(),
                id: id.into(),
            }
        } else {
            Route::set(id)
        }
    }
}

fn main() {
    // One MCP server: Studio's own, which also carries the engine's
    // actions (`hosted::forward_engine_actions`). The engine Studio starts
    // keeps its list as Studio's private line and publishes no discovery
    // file of its own (`studio_engine::mcp::PRIVATE`). Set before any
    // thread starts, so every process Studio starts inherits it.
    #[cfg(feature = "native")]
    // SAFETY: the first statement of `main`, before any thread exists.
    unsafe {
        std::env::set_var("FTS_MCP_PRIVATE", "1");
        // One engine: the daw extension's launcher looks where Studio
        // starts its engine (`engine::engine_dir`).
        if std::env::var_os("FTS_DAW_ENGINE_DIR").is_none()
            && let Some(dir) = engine::engine_dir()
        {
            std::env::set_var("FTS_DAW_ENGINE_DIR", dir);
        }
        // Its private line on a port of its own, whichever side starts it:
        // another engine on the default port must not leave it unreachable.
        if std::env::var_os("FTS_MCP_PORT").is_none()
            && let Some(port) = engine::free_port()
        {
            std::env::set_var("FTS_MCP_PORT", port.to_string());
        }
    }
    storage::init();
    // Natively: stderr and, when OTEL_EXPORTER_OTLP_TRACES_ENDPOINT is set,
    // the app's spans to that collector.
    #[cfg(feature = "native")]
    let _telemetry =
        app_shell_native::telemetry::init("studio-app", "warn,studio_app=info,drive_dioxus=info");
    // Warnings and up: Loro's encoder logs every block at info.
    #[cfg(not(feature = "native"))]
    dioxus::logger::init(dioxus::logger::tracing::Level::WARN).ok();
    // Served under a path (`--base-path studio`), the node is reached
    // under it too (`/studio/api/…`), so a site can hand Studio that path.
    #[cfg(feature = "web")]
    drive_dioxus::mount::set(&dioxus::cli_config::web_base_path().unwrap_or_default());
    // Drawn by Blitz on a desktop: the window has no system frame, the
    // shell's title bar is its frame (app-shell-native).
    #[cfg(feature = "native")]
    app_shell_native::launch(
        App,
        popout,
        app_shell_native::Config {
            title: "Studio".to_owned(),
            // What the window manager's rules match (Loft's: `^Studio$`).
            app_id: Some("Studio".to_owned()),
            size: (1440.0, 900.0),
            system_decorations: false,
            // `shot <path.png>` on $XDG_RUNTIME_DIR/app-shell/studio.sock:
            // the app paints its window to a file (development, agents).
            control: Some("studio".to_owned()),
        },
    );
    // The window closed: the engine Studio started goes with it, unless
    // Studio keeps it (a development build, or STUDIO_KEEP_ENGINE=1).
    #[cfg(feature = "native")]
    engine::stop_started_engine();
    #[cfg(not(feature = "native"))]
    dioxus::launch(App);
}

/// The session's other windows (the Stage's lighting and chart): windows
/// of the session's layout, drawing their extension views as the main
/// window does.
#[cfg(feature = "native")]
fn popout(id: app_shell::WindowId) -> Element {
    // The launcher, in a window of its own while a session plays.
    if id.0 == stage::LAUNCHER_WINDOW {
        return rsx! { LauncherApp {} };
    }
    // A document of its own, without the app's sheets: sized inline.
    rsx! {
        div { style: "position:absolute; top:0; left:0; width:100vw; height:100vh; display:flex; flex-direction:column;",
            app_shell_extensions::ExtensionPopout {
                window: id,
                theme: crate::shell::theme(),
                tabs: app_shell::ui::Tabs::On,
            }
        }
    }
}

/// The launcher's own window during a session (its context): it browses
/// and plays nothing itself; the session and the engine are the main
/// window's.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct LauncherWindow;

/// Whether this document is the launcher's own window.
pub(crate) fn in_launcher_window() -> bool {
    try_use_context::<LauncherWindow>().is_some()
}

/// The launcher's window: the app, as a window titled "Launcher".
#[cfg(feature = "native")]
#[component]
fn LauncherApp() -> Element {
    use_context_provider(|| LauncherWindow);
    rsx! { App {} }
}

#[component]
fn App() -> Element {
    use_context_provider(state::AppState::new);
    let title = if in_launcher_window() {
        "Launcher"
    } else {
        "Studio"
    };
    // From navigation until the app has first drawn.
    #[cfg(feature = "drive")]
    use_effect(trace::boot);
    rsx! {
        document::Meta {
            name: "viewport",
            content: "width=device-width, initial-scale=1, viewport-fit=cover",
        }
        document::Meta { name: "theme-color", content: "#000000" }
        document::Title { "{title}" }
        document::Link { rel: "preconnect", href: "https://fonts.googleapis.com" }
        document::Stylesheet { href: FONT }
        document::Stylesheet { href: STYLE }
        document::Stylesheet { href: APP_STYLE }
        live_sync::Provider { Router::<Route> {} }
    }
}
