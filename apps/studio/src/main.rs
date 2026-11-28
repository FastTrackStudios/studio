//! Studio: one Dioxus app for the phone, the desktop and the web. What it
//! shows is real: the setlist files and records in Studio's home and
//! Drive (`drive_sets`, `records`), the org's sets, library and songs as
//! Drive documents, each song a session (`songs`, on `session_sync`);
//! each org has its own library, team and settings, and shares songs with
//! others (`orgs`, `team`); its sets are Drive documents too (`setlist`
//! the model, `sets` live). Where there is nothing yet, a page says how
//! to add the real thing.

mod actions;
mod day;
mod door;
#[cfg(feature = "drive")]
mod drive_sets;
#[cfg(feature = "native")]
mod engine;
// Every page's data from an empty home, and from one laid out as storage.md says.
#[cfg(test)]
mod empty_home;
// The session's views: the real extensions, hosted (natively).
mod files;
mod files_live;
// From signing in to an open setlist on a fresh install (studio#699).
mod first_run;
#[cfg(feature = "native")]
mod hosted;
mod icons;
mod instance;
mod keys;
// What a signed-out visitor to fasttrackstudio.app sees (the web build's).
mod landing;
mod live_sync;
mod look;
mod new_session;
#[cfg(any(feature = "drive", test))]
mod org_create;
#[cfg(any(feature = "drive", test))]
mod org_members;
mod orgs;
mod pages;
mod pieces;
// Records: the studio-kind files, each an album and its tracks.
#[cfg(feature = "native")]
mod playback;
#[cfg(feature = "web")]
mod player;
#[cfg_attr(not(feature = "drive"), allow(dead_code))]
mod records;
// The engine in the page: a session's multitracks and click (web).
#[cfg(feature = "web")]
mod engine_web;
#[cfg(feature = "drive")]
mod offline;
// Desktop Studio updating itself: macOS and Linux (iOS: TestFlight).
#[cfg(all(feature = "native", not(target_os = "ios")))]
mod self_update;
mod storage;
// The views' theme packs, switched while the app runs (`THEME_*`).
#[cfg(feature = "native")]
mod themes;
#[cfg(feature = "drive")]
mod trace;
// The browser build engraves charts and reads setlist files from Drive.
#[cfg_attr(not(feature = "drive"), allow(dead_code))]
mod chart_sync;
// The chart a song with no chart shows, generated from its session (web).
#[cfg_attr(not(feature = "web"), allow(dead_code))]
mod chart_generated;
#[cfg_attr(not(feature = "web"), allow(dead_code))]
mod reference;
#[cfg(feature = "native")]
mod room_link;
// Where what a room loaded is, read by path (a browser's member).
#[cfg_attr(not(feature = "web"), allow(dead_code))]
mod room_paths;
// A browser in a room: the page joins it and follows its leader (web).
#[cfg(feature = "web")]
mod room_web;
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
mod song;
mod song_ids;
mod songs;
#[cfg(feature = "native")]
mod stage;
mod state;
mod team;
#[cfg(feature = "native")]
mod telemetry;

use dioxus::prelude::*;

use door::LandingPage;
use pages::drive_set::{DriveSet, NowChart};
use pages::home::Home;
use pages::join::JoinRoom;
use pages::library::{Library, LibraryHome};
use pages::live::{Chart, Lyrics};
use pages::orgs::{AllOrgs, OrgSettings, People};
use pages::print::PrintSet;
use pages::records::{Album, Records};
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
///
/// Each tab has its own path (`/home`, `/records`, `/setlists`,
/// `/signal`); `/` is replaced by `/home` (the router puts the canonical
/// path in the address bar in place, so Back does not loop). `/landing`
/// is the web build's landing page, signed in (`door::LandingPage`); the
/// desktop and iOS builds go home from it.
#[derive(Clone, Debug, PartialEq, Routable)]
#[rustfmt::skip]
pub enum Route {
    #[redirect("/", || Route::Home {})]
    #[route("/landing")]
    LandingPage {},
    #[layout(Root)]
    #[layout(Shell)]
        #[route("/home")]
        Home {},
        #[route("/records")]
        Records {},
        #[route("/records/:id")]
        Album { id: String },
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
        // A room's lasting link (`fasttrackstudio.app/join/<room>`).
        #[route("/join/:room")]
        JoinRoom { room: String },
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
            Route::Records {} | Route::Album { .. } => Tab::Records,
            Route::Signal {} => Tab::Signal,
            Route::LandingPage {}
            | Route::AllOrgs {}
            | Route::People {}
            | Route::OrgSettings {}
            | Route::JoinRoom { .. } => Tab::Home,
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
}

fn main() {
    // `--version`: the release's version and nothing else, before any
    // window, engine, audio or log starts. The release's smoke test reads
    // it headless (scripts/desktop/linux-smoke.sh expects `studio X.Y.Z`),
    // and it is how a user tells which Studio they have.
    // In a browser, a panic before the app renders (its crash span's hook)
    // is said on the console, not lost to `unreachable`.
    #[cfg(feature = "web")]
    console_error_panic_hook::set_once();
    if std::env::args().nth(1).as_deref() == Some("--version") {
        use std::io::Write as _;
        let _ = writeln!(
            std::io::stdout(),
            "studio {}",
            fts_version::release_version()
        );
        return;
    }
    // One MCP server: Studio's own, which also carries the engine's
    // actions (`hosted::forward_engine_actions`). The engine Studio starts
    // keeps its list as Studio's private line and publishes no discovery
    // file of its own (`studio_engine::mcp::PRIVATE`). Set before any
    // thread starts, so every process Studio starts inherits it.
    #[cfg(feature = "native")]
    // SAFETY: the first statement of `main`, before any thread exists.
    unsafe {
        std::env::set_var("FTS_MCP_PRIVATE", "1");
        // The release's version, for the engine Studio starts to report
        // (daw-extension reads it; it cannot depend on fts-version).
        std::env::set_var("STUDIO_APP_VERSION", fts_version::release_version());
        // One engine: the daw extension's launcher looks where Studio
        // starts its engine (`engine::engine_dir`).
        if std::env::var_os("FTS_DAW_ENGINE_DIR").is_none()
            && let Some(dir) = engine::engine_dir()
        {
            std::env::set_var("FTS_DAW_ENGINE_DIR", dir);
        }
        // A fresh install names no Drive node: FastTrack's, which signs in
        // through the account server (`first_run`). Anything that names
        // one (DRIVE_NODE, the Drive agent, a node on this computer) wins.
        if let Some(node) = first_run::node_to_name() {
            std::env::set_var("DRIVE_NODE", node);
        }
        // Its private line on a port of its own, whichever side starts it:
        // another engine on the default port must not leave it unreachable.
        if std::env::var_os("FTS_MCP_PORT").is_none()
            && let Some(port) = engine::free_port()
        {
            std::env::set_var("FTS_MCP_PORT", port.to_string());
        }
        // On iPhone and iPad the engine is this process: it keeps and
        // finds rooms in the app's own Studio folder, beside the setlists
        // a room loads, not in a Drive mount a phone does not have.
        #[cfg(target_os = "ios")]
        if std::env::var_os(room::record::ROOMS_DIR_ENV).is_none()
            && let Some(dir) = room_link::rooms_dir(storage::home().as_deref())
        {
            std::env::set_var(room::record::ROOMS_DIR_ENV, dir);
        }
    }
    // The Editor: what the arrangement picks opens there, and a key
    // sequence it has half typed keeps its keys from the panes.
    #[cfg(feature = "native")]
    midi_editor::install();
    // The views' keys: Studio's input profile.
    #[cfg(feature = "native")]
    studio_panels::hooks::install();
    // The release's version, for whatever reports it (the app's spans and
    // crash reports, the update check, the MCP server).
    utils::version::set(fts_version::release_version());
    storage::init();
    // Natively: stderr and, when a collector is named or a release baked
    // one in, the app's spans and crash reports to it.
    #[cfg(feature = "native")]
    {
        telemetry::init();
        if std::env::args().nth(1).as_deref() == Some("--crash-test") {
            telemetry::crash_test();
        }
        // `--join <link>`: into that room as soon as the engine is up.
        if let Some(link) = room_link::link_in(&std::env::args().collect::<Vec<_>>()) {
            room_link::join(link, room_link::member_name());
        }
    }
    // Warnings and up: Loro's encoder logs every block at info.
    #[cfg(not(feature = "native"))]
    dioxus::logger::init(dioxus::logger::tracing::Level::WARN).ok();
    // The node is reached under the path it serves Studio at
    // (`/studio/api/…`): the page's own path when served under one
    // (`--base-path studio`), and `/studio` from the root of
    // fasttrackstudio.app, whose front hands `/studio/api` to the node
    // (door.rs).
    // The page's address as it loaded: the router replaces it with the
    // route it shows, and `?session=` goes with it.
    #[cfg(feature = "web")]
    engine_web::remember_address();
    // The page keeps its own files for opening with no network
    // (`public/sw.js`), under the path it is served at.
    #[cfg(feature = "web")]
    engine_web::register_service_worker(&dioxus::cli_config::web_base_path().unwrap_or_default());
    // The session's arrangement and mixer in the page: installed for the
    // panels that show them (`studio_panels::web_host::HostedView`),
    // before the first one draws.
    #[cfg(feature = "web")]
    {
        arrangement_view::arrangement_host::install();
        mixer_view::mixer_host::install();
    }
    #[cfg(feature = "web")]
    drive_dioxus::mount::set(door::node_mount(
        &dioxus::cli_config::web_base_path().unwrap_or_default(),
    ));
    // iOS 27 stops an app that does not adopt scenes at launch, and winit
    // does not adopt them: the app's own delegate, named in Info.plist,
    // registered before UIKit looks it up (the window is handed to it in
    // `App`).
    #[cfg(all(feature = "native", target_os = "ios"))]
    {
        // A room's link the app is opened with (`fasttrackstudio://join/…`,
        // the URL type ios/build.sh declares) joins that room: before the
        // delegate is registered, so the link it was launched by is seen.
        studio_shell::ios_scene::on_link(room_link::link_opened);
        studio_shell::ios_scene::register(studio_shell::ios_scene::Opening::AsHeld);
    }
    // Drawn by Blitz on a desktop: the window has no system frame, the
    // shell's title bar is its frame (app-shell-native).
    #[cfg(feature = "native")]
    app_shell_native::launch(
        App,
        popout,
        app_shell_native::Config {
            title: instance::title(),
            // What the window manager's rules match (Loft's: `^Studio$`):
            // "Studio" for every instance.
            app_id: Some(instance::app_id()),
            size: (1440.0, 900.0),
            system_decorations: false,
            // `shot <path.png>` on $XDG_RUNTIME_DIR/app-shell/studio.sock:
            // the app paints its window to a file (development, agents).
            control: Some(instance::name().to_owned()),
        },
    );
    // The window closed: the engine Studio started goes with it, unless
    // Studio keeps it (a development build, or STUDIO_KEEP_ENGINE=1).
    #[cfg(feature = "native")]
    engine::stop_started_engine();
    // What the exporter still holds, sent before the process ends.
    #[cfg(feature = "native")]
    studio_otel::flush();
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
    // The Action List's doors from the start, Drive's sign-in screen
    // included: an agent can ask who this is and sign in (STUDIO_ACCOUNT,
    // STUDIO_SIGN_IN) before the app's pages exist.
    #[cfg(feature = "native")]
    use_hook(actions::open_doors_once);
    // iOS: winit's window, handed to the app's scene (studio-shell's
    // `ios_scene`); a second window's handover is a no-op.
    #[cfg(all(feature = "native", target_os = "ios"))]
    {
        let handle = dioxus::native::use_raw_window_handle();
        use_hook(move || {
            if let raw_window_handle::RawWindowHandle::UiKit(view) = handle {
                studio_shell::ios_scene::window_created(view.ui_view);
            }
            // The engine is the app's on a phone, started with it: the
            // extensions set up now (the linked daw extension starts the
            // engine), not when a session first opens as on a desktop.
            hosted::extensions();
        });
    }
    let title = if in_launcher_window() {
        "Launcher"
    } else {
        "Studio"
    };
    // From navigation until the app has first drawn.
    #[cfg(feature = "drive")]
    use_effect(trace::boot);
    // In a browser, a panic is sent as a crash span (after the renderer's
    // own panic hook is in, so both run).
    #[cfg(all(feature = "drive", target_arch = "wasm32"))]
    use_hook(trace::install_crash_hook);
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
