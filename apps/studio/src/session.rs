//! The session: a set open for playing, taking the whole window. The
//! app's other mode beside the launcher (Home, Setlists, the set pages and
//! the sidebar), as the old app switched from its launcher into Session's
//! frame (`features/engine/view-kit/src/shell.rs`).
//!
//! Its own layout (`Route::SessionPage`, under the root beside the
//! launcher's `Shell`): an `AppShell` of its own with no sidebars. Natively
//! it is the old Session window's, made of the same components: daw-view-kit's
//! `TopBar` (the navigator, the set's song tabs, Save and the checkpoints)
//! across the top bar's middle and its `BottomBar` (Setup, the views, the
//! transport, the modes) as the accessory, fed by the engine the app drives
//! as the old window was fed by `studio engine`. The middle is one pane of
//! the session's layout: the real arrangement extension's view first
//! ([`crate::hosted`]), the lyrics extension's, or the app's own pages
//! (the chart, the overview). The mixer is the mixer extension's view in
//! a window of its own. Playback is the root's, so the set plays on
//! across a switch back to the launcher and in again.
//!
//! In a browser the session's views are tabs over its stage
//! ([`WEB_VIEWS`]). The arrangement, the mixer and the editor are the same
//! panels the desktop draws, over the engine in the page
//! (`studio_panels::web_host::HostedView`, fed by `crate::engine_web`).

use dioxus::prelude::*;

#[cfg(not(feature = "native"))]
use app_shell::ui::{AppShell, TopBar};

use crate::Route;
use crate::song::Song;
use crate::state::{AppState, Place, use_app};

/// The session's views, as the old bottom bar had them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Overview,
    Chart,
    Lyrics,
    Editor,
    Arrangement,
    Mixer,
    Fx,
    Setup,
    /// Ignition's visualizer: the show's lighting, following the song.
    Lighting,
}

impl View {
    pub const ALL: [View; 9] = [
        View::Overview,
        View::Chart,
        View::Lyrics,
        View::Editor,
        View::Arrangement,
        View::Mixer,
        View::Fx,
        View::Setup,
        View::Lighting,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            View::Overview => "overview",
            View::Chart => "chart",
            View::Lyrics => "lyrics",
            View::Editor => "editor",
            View::Arrangement => "arrangement",
            View::Mixer => "mixer",
            View::Fx => "fx",
            View::Setup => "setup",
            View::Lighting => "lighting",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            View::Overview => "Overview",
            View::Chart => "Chart",
            View::Lyrics => "Lyrics",
            View::Editor => "Editor",
            View::Arrangement => "Arrangement",
            View::Mixer => "Mixer",
            View::Fx => "FX",
            View::Setup => "Setup",
            View::Lighting => "Lighting",
        }
    }

    /// A view by its slug (`daw` is the arrangement, as the old app named it).
    pub fn parse(s: &str) -> Option<View> {
        let s = s.trim().to_ascii_lowercase();
        if s == "daw" {
            return Some(View::Arrangement);
        }
        View::ALL.into_iter().find(|v| v.slug() == s)
    }
}

/// The screensets, by number and name, and the view each brings up:
/// `SESSION_SCREENSET` in Studio's window (the engine's own recalls the old
/// Session window's dock).
pub const SCREENSETS: [(u8, &str, View); 7] = [
    (1, "Performance", View::Lighting),
    (2, "Overview", View::Overview),
    (3, "Session", View::Arrangement),
    (4, "Mix", View::Mixer),
    (5, "Edit", View::Editor),
    (6, "Charts", View::Chart),
    (7, "Signal", View::Fx),
];

/// The screenset `name` names (its number or its name, any case): its name
/// and its view.
///
/// # Errors
///
/// No screenset is called that.
pub fn screenset(name: &str) -> Result<(&'static str, View), String> {
    let name = name.trim();
    SCREENSETS
        .iter()
        .find(|(n, s, _)| name == n.to_string() || name.eq_ignore_ascii_case(s))
        .map(|(_, s, v)| (*s, *v))
        .ok_or_else(|| {
            let all: Vec<String> = SCREENSETS
                .iter()
                .map(|(n, s, _)| format!("{n} {s}"))
                .collect();
            format!("no screenset {name}: one of {}", all.join(", "))
        })
}

/// The session's views in a browser, as its tabs: the overview, the
/// chart, and the arrangement, the mixer and the editor over the page's
/// engine.
#[cfg_attr(feature = "native", allow(dead_code))]
pub const WEB_VIEWS: [View; 5] = [
    View::Overview,
    View::Chart,
    View::Arrangement,
    View::Mixer,
    View::Editor,
];

/// Where a view of the session is.
pub fn route(view: View) -> Route {
    Route::SessionPage {
        view: view.slug().into(),
    }
}

/// Into the session: song `i` of a set's songs on show (playing or not),
/// "back to sets" returning to `from`.
#[cfg_attr(not(feature = "drive"), allow(dead_code))]
pub fn enter(
    app: AppState,
    list: Vec<(Song, Place)>,
    name: String,
    i: usize,
    play: bool,
    from: Option<Route>,
    view: View,
) {
    #[cfg(feature = "drive")]
    let _span = {
        let mut span = crate::trace::Span::root("studio.session.open");
        span.set("studio.set", name.clone());
        span.set("studio.set.songs", list.len() as u64);
        if let Some((s, _)) = list.get(i) {
            span.set("studio.song", s.title);
        }
        span.set("studio.session.view", view.slug());
        span
    };
    app.select_places(list, name, i, play);
    if let Some(f) = from {
        let mut back = app.session_from;
        back.set(Some(f));
    }
    navigator().push(route(view));
}

/// The mode asked for on Studio's Action List, until the session takes it.
#[cfg(feature = "native")]
static ASKED_MODE: std::sync::Mutex<Option<engine_domain::modes::Mode>> =
    std::sync::Mutex::new(None);

/// Asks this Studio's windows for mode `name` (`SESSION_MODE`): its name as
/// the bars show it.
///
/// # Errors
///
/// No mode is called that.
#[cfg(feature = "native")]
pub fn ask_mode(name: &str) -> Result<&'static str, String> {
    let mode = engine_domain::modes::Mode::ALL
        .into_iter()
        .find(|m| m.display_name().eq_ignore_ascii_case(name.trim()))
        .ok_or_else(|| {
            let names: Vec<&str> = engine_domain::modes::Mode::ALL
                .iter()
                .map(|m| m.display_name())
                .collect();
            format!("no mode {name}: one of {names:?}")
        })?;
    *ASKED_MODE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(mode);
    Ok(mode.display_name())
}

#[cfg(feature = "native")]
fn take_asked_mode() -> Option<engine_domain::modes::Mode> {
    ASKED_MODE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
}

/// Back to the launcher, where the session was opened from. The set plays on.
#[cfg_attr(not(feature = "native"), allow(dead_code))]
pub fn leave(app: AppState) {
    let to = app
        .session_from
        .peek()
        .clone()
        .unwrap_or(Route::Setlists {});
    // Natively the Stage's other windows close first, while the session's
    // shell is there to close them; the engine keeps the set.
    #[cfg(feature = "native")]
    if crate::hosted::close_stage_windows() {
        spawn(async move {
            crate::state::sleep_ms(120).await;
            navigator().push(to);
        });
        return;
    }
    navigator().push(to);
}

/// Song `j` of the set, from its start, playing as the set was.
pub fn go_to(app: AppState, j: usize) {
    let mut np = app.np;
    np.with_mut(|n| {
        if let Some(n) = n
            && j < n.list.len()
            && j != n.i
        {
            n.i = j;
            n.pos = 0.0;
            n.dur = 0.0;
            n.timeline = Default::default();
        }
    });
}

/// The session's frame: its own shell, no sidebars.
#[component]
pub fn SessionShell() -> Element {
    let app = use_app();
    let route = use_route::<Route>();
    let view = match &route {
        Route::SessionPage { view } => View::parse(view).unwrap_or(View::Arrangement),
        _ => View::Arrangement,
    };
    let title = (app.np)()
        .map(|n| n.from.clone())
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| "Session".into());
    #[cfg(feature = "native")]
    {
        if crate::in_launcher_window() {
            return rsx! {
                div { class: "sessempty",
                    b { "{title}" }
                    p { class: "pline", "The session is in Studio's main window." }
                }
            };
        }
        rsx! {
            native::Session { view, title }
        }
    }
    #[cfg(not(feature = "native"))]
    {
        // The song playing, its transport under the views: at the foot of a
        // phone (in thumb's reach) or under the stage on a wider screen; a
        // tap opens the player, as in the launcher.
        let accessory = (app.np)()
            .is_some_and(|n| n.song().is_some())
            .then(|| rsx! { crate::shell::NowPlayingBar {} });
        rsx! {
            div { class: "window",
                AppShell {
                    theme: use_hook(crate::shell::theme),
                    title: title.clone(),
                    top_bar: rsx! {
                        // Back is home: the launcher (⌘⇧H, STUDIO_HOME).
                        TopBar { title: title.clone(), on_back: move |()| leave(app) }
                    },
                    accessory,
                    crate::shell::Content { class: "session",
                        main { class: "sessstage",
                            nav { class: "sesstabs", aria_label: "The session's views",
                                for v in WEB_VIEWS {
                                    button {
                                        key: "{v.slug()}",
                                        class: "sesstab",
                                        "data-view": v.slug(),
                                        "aria-current": if v == view { "page" } else { "false" },
                                        onclick: move |_| {
                                            navigator().replace(self::route(v));
                                        },
                                        "{v.name()}"
                                    }
                                }
                            }
                            Outlet::<Route> {}
                        }
                    }
                    crate::shell::SheetBridge {}
                    crate::shell::ToastBridge {}
                }
            }
        }
    }
}

/// A view of the session, by its route: the app's own pages. Natively the
/// session draws its views in its layout's pane ([`page`]); a route still
/// names the one on show.
#[component]
pub fn SessionPage(view: String) -> Element {
    page(view)
}

/// The app's own page for view `slug`: the chart, the overview, or a word
/// for a view drawn elsewhere (natively, the arrangement and the lyrics are
/// their extensions', the mixer a window of its own).
pub fn page(slug: String) -> Element {
    let v = View::parse(&slug).unwrap_or(View::Arrangement);
    let body = match v {
        View::Chart => rsx! {
            crate::pages::drive_set::ChartView {}
        },
        View::Overview => rsx! {
            Overview {}
        },
        // daw-view-kit's own panels over the engine's song, as the old
        // Session window showed them (no extension of their own yet).
        #[cfg(feature = "native")]
        View::Setup | View::Editor | View::Fx => rsx! {
            native::SongPanel { view: v }
        },
        // The page's engine's own panels: its song's tracks, items,
        // sections, automation and takes; every strip's fader, pan, mute,
        // solo, sends, FX and Signal blocks.
        // And the Editor: the expression editor on the item the arrangement
        // picked, written back to the engine in the page.
        #[cfg(feature = "web")]
        View::Arrangement | View::Mixer | View::Editor => {
            use studio_panels::web_host::{HostedPanel, HostedView};
            let panel = match v {
                View::Mixer => HostedPanel::Mixer,
                View::Editor => HostedPanel::Editor,
                _ => HostedPanel::Arrangement,
            };
            return rsx! {
                div { class: "sessview panes",
                    div { class: "sesspane", key: "{v.slug()}",
                        HostedView { panel }
                    }
                }
            };
        }
        other => rsx! {
            div { class: "sessempty",
                b { "{other.name()}" }
                p { class: "pline",
                    if cfg!(feature = "native") {
                        "Not in this Studio yet."
                    } else {
                        "This view is the desktop app's for now; the overview, the chart, the arrangement, the mixer and the editor are here."
                    }
                }
            }
        },
    };
    rsx! {
        div { class: "sessview", {body} }
    }
}

/// The set at a glance: every song, its length, where the set is.
#[component]
fn Overview() -> Element {
    let app = use_app();
    let Some(n) = (app.np)() else {
        return rsx! {
            p { class: "pline", "No set is open." }
        };
    };
    let len = |s: &Song| f64::from(s.len.unwrap_or(0));
    let total: f64 = n.list.iter().map(len).sum();
    rsx! {
        div { class: "sessover",
            h2 { "{n.from}" }
            p { class: "pline",
                "{n.list.len()} songs · {crate::songs::clock(total)} · {(app.engine)().line()}"
            }
            for (j , s) in n.list.iter().enumerate() {
                button {
                    key: "{j}",
                    class: if j == n.i { "sessrow on" } else { "sessrow" },
                    onclick: move |_| go_to(app, j),
                    span { class: "sessnum", "{j + 1}" }
                    span { class: "sesst", "{s.title}" }
                    span { class: "sesslen", "{crate::songs::clock(len(s))}" }
                }
            }
        }
    }
}

/// The session natively: daw-view-kit's bars around the session's layout,
/// whose pane is a hosted extension's view or the app's page.
#[cfg(feature = "native")]
mod native {
    use dioxus::prelude::*;

    use app_shell::Workspace;
    use app_shell::ui::{AppShell, BarSpan, Tabs, TopBar, use_top_bar_span};
    use app_shell_extensions::{ExtensionOverlays, ExtensionUi, use_extension_ui};
    use engine_client::setlist::Setlist;
    use studio_panels::shell::View as Ui;

    use super::{View, go_to, leave, route};
    use crate::hosted;
    use crate::state::use_app;

    /// The app's view as daw-view-kit names it.
    pub(super) const fn to_ui(v: View) -> Ui {
        match v {
            View::Overview => Ui::Overview,
            View::Chart => Ui::Chart,
            View::Lyrics => Ui::Lyrics,
            View::Editor => Ui::Editor,
            View::Arrangement => Ui::Daw,
            View::Mixer => Ui::Mixer,
            View::Fx => Ui::Fx,
            View::Setup => Ui::Setup,
            // No button of its own in daw-view-kit's bar: Performance, which
            // this app has none of, stands for it while it shows.
            View::Lighting => Ui::Performance,
        }
    }

    /// daw-view-kit's view as the app's (Performance is the overview here).
    pub(super) const fn of_ui(v: Ui) -> View {
        match v {
            Ui::Performance => View::Lighting,
            Ui::Overview => View::Overview,
            Ui::Chart => View::Chart,
            Ui::Lyrics => View::Lyrics,
            Ui::Editor => View::Editor,
            Ui::Daw => View::Arrangement,
            Ui::Mixer => View::Mixer,
            Ui::Fx => View::Fx,
            Ui::Setup => View::Setup,
        }
    }

    /// The app's own page the main window shows for view `v`, when it is
    /// not one of the Stage's (the arrangement and mixer, the chart and
    /// lyrics, the lighting: always on show in their windows).
    fn page_for(v: View) -> Option<app_shell::Pane> {
        match v {
            View::Arrangement | View::Mixer | View::Chart | View::Lyrics | View::Lighting => None,
            other => {
                let mut pane = app_shell::Pane::new(
                    format!("{}:{}", hosted::PAGE, other.slug()),
                    hosted::PAGE,
                    other.name(),
                );
                pane.state = other.slug().to_owned();
                Some(pane)
            }
        }
    }

    /// The main window's area now (app-shell-native's measure), else the
    /// display the Stage was first laid out on.
    fn area_now() -> crate::stage::Area {
        try_consume_context::<app_shell::ui::ViewportSize>()
            .map(|v| *v.0.peek())
            .filter(|(w, h)| *w > 0.0 && *h > 0.0)
            .map_or_else(crate::stage::Area::default, |(w, h)| crate::stage::Area {
                w,
                h,
            })
    }

    /// The Stage for `view`, its chart on `followed`'s session; in one
    /// window ([`crate::instance::single_window`]), its main window alone.
    fn stage_for(view: View, set: &str, followed: Option<&hosted::Followed>) -> Workspace {
        let names = crate::stage::Names {
            set: set.to_owned(),
            chart_state: followed
                .map(|f| format!("session:{}", f.id))
                .unwrap_or_default(),
            kind: crate::setfile::kind_of(set),
        };
        let single = crate::instance::single_window();
        crate::stage::with_centre(area_now(), &names, centre(view), page_for(view), !single)
    }

    /// The Stage pane `view` fills the main window with: the chart (the
    /// Charts screenset) always; in one window the lyrics and the lighting
    /// too. `None`: the Stage's own main window, or the app's page.
    fn centre(view: View) -> Option<&'static str> {
        match view {
            View::Chart => Some(crate::stage::CHART),
            View::Lyrics if crate::instance::single_window() => Some(crate::stage::LYRICS),
            View::Lighting if crate::instance::single_window() => Some(crate::stage::SHOW),
            _ => None,
        }
    }

    /// The main window's first pane when it shows `view`, when that is not
    /// the Stage's own: what tells whether it must be laid out again.
    fn centre_key(view: View) -> Option<String> {
        centre(view)
            .map(str::to_owned)
            .or_else(|| page_for(view).map(|p| p.id.0))
    }

    /// Whether `pane`, first in the main window, is one [`centre_key`]
    /// names (the Stage's own main window starts with the progress bar).
    fn is_centre(pane: &str) -> bool {
        pane.starts_with(hosted::PAGE)
            || [
                crate::stage::CHART,
                crate::stage::LYRICS,
                crate::stage::SHOW,
            ]
            .contains(&pane)
    }

    /// daw-view-kit's Setup, Editor or FX panel over the song the engine has
    /// (its session in context, as the old window's `SongViews` gave it).
    #[component]
    pub fn SongPanel(view: View) -> Element {
        let song = studio_view_link::use_song();
        let Some((key, session)) = song() else {
            return studio_view_link::waiting();
        };
        rsx! {
            div { style: "position:relative; flex:1; min-height:0; height:100%;",
                for key in std::iter::once(key) {
                    WithSong { key: "{key}", session: Song(session.clone()),
                        match view {
                            View::Setup => rsx! { studio_panels::setup::SetupView {} },
                            View::Editor => rsx! { midi_editor::editor_view::EditorPanel {} },
                            _ => rsx! { mixer_view::fx_view::FxPanel { lanes: 4 } },
                        }
                    }
                }
            }
        }
    }

    /// The engine's set, as daw-view-kit's song tabs read it (a signal of
    /// the setlist, as the old window provided it): followed over the
    /// views' link, changed when the engine's set does.
    fn use_engine_setlist() -> Signal<Setlist> {
        let now = || {
            engine_client::view_client::peek_set(|v| {
                (v.revision, v.setlist.clone().unwrap_or_default())
            })
        };
        let mut setlist = use_context_provider(|| Signal::new(now().1));
        use_future(move || async move {
            let mut seen = u64::MAX;
            loop {
                let revision = engine_client::view_client::peek_set(|v| v.revision);
                if revision != seen {
                    seen = revision;
                    setlist.set(now().1);
                }
                crate::state::sleep_ms(200).await;
            }
        });
        setlist
    }

    #[component]
    pub fn Session(view: View, title: String) -> Element {
        let app = use_app();
        hosted::use_engine_link();
        // The Stage: three windows (main, lighting, chart), all opened
        // with the session.
        let layout = use_signal(|| stage_for(view, &title, None));
        let ui = use_extension_ui(hosted::host(), layout);
        use_hook(move || hosted::set_session_ui(Some(ui)));
        use_drop(|| hosted::set_session_ui(None));
        // Closing the main window goes home to the launcher, not out of
        // the app (the shell's own close button reads these controls).
        if let Some(controls) = try_use_context::<app_shell::ui::WindowControls>() {
            use_context_provider(move || app_shell::ui::WindowControls {
                close: Some(Callback::new(move |()| leave(app))),
                ..controls
            });
        }
        // Process views (Ignition's visualizer) drawn in the window's panes: a
        // program of their own, so only where the app may start one (not
        // iPhone or iPad).
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        use_context_provider({
            let views = architect_process_view::extension::use_process_views(hosted::host());
            move || views
        });
        // What daw-view-kit's bars and panels read, as the old Session
        // window gave it them: the mode, the pins, touch, the set, the
        // mixer's memory, the close-ups, the lyrics' choice, and Back.
        let mode = use_context_provider(|| Signal::new(studio_view_link::mode_from_env()));
        // A mode asked for on Studio's Action List (`SESSION_MODE` over MCP,
        // a script), taken up within a tenth of a second. The mode is this
        // client's: the engine never sees it.
        let mut asked = mode;
        use_future(move || async move {
            loop {
                if let Some(next) = super::take_asked_mode() {
                    asked.set(next);
                }
                crate::state::sleep_ms(100).await;
            }
        });
        studio_panels::shell::use_pins();
        use_context_provider(daw_view_kit::touch::Touch::detect);
        use_context_provider(mixer_view::mixer_panel::MixerMemory::new);
        use_context_provider(mixer_view::closeup::Closeups::new);
        use_context_provider(studio_panels::lyrics_panel::LyricsChoice::new);
        use_context_provider(|| studio_panels::shell::Back(Callback::new(move |()| leave(app))));
        use_engine_setlist();
        // Keyflow's chart and Ignition's visualizer follow the engine's song.
        hosted::use_engine_actions();
        let followed = hosted::use_follow_session(ui, layout);
        // The mixer beside the arrangement or under it as the main window
        // is wide or not: laid out again when that changes.
        if let Some(viewport) = try_use_context::<app_shell::ui::ViewportSize>() {
            let wide = move || {
                let (w, h) = viewport.0();
                h > 0.0 && w / h >= crate::stage::WIDE
            };
            let mut was = use_signal(|| None::<bool>);
            let set_now = title.clone();
            use_effect(move || {
                let now = wide();
                let before = *was.peek();
                was.set(Some(now));
                if before.is_some_and(|b| b != now) && page_for(view).is_none() {
                    let next = stage_for(view, &set_now, followed.peek().as_ref());
                    ui.apply(move |w| {
                        let mut next = next;
                        crate::stage::carry_rig(w, &mut next);
                        *w = next;
                        Ok::<_, app_shell::WorkspaceError>(())
                    });
                }
            });
        }
        // The bottom bar's view: the route's, and a press there goes to
        // its route (the mixer is beside the arrangement: the view stays).
        let mut shown = use_signal(|| to_ui(view));
        let set_title = title.clone();
        use_effect(use_reactive!(|view, set_title| {
            if *shown.peek() != to_ui(view) {
                shown.set(to_ui(view));
            }
            // The main window's centre: the Stage's, or the app's page;
            // the other windows as they are.
            let want = centre_key(view);
            let now = layout
                .peek()
                .windows
                .first()
                .and_then(app_shell::Window::first_group)
                .and_then(|g| g.current_tab().cloned())
                .map(|p| p.0);
            if want != now.filter(|p| is_centre(p)) {
                let next = stage_for(view, &set_title, followed.peek().as_ref());
                ui.apply(move |w| {
                    let mut next = next;
                    crate::stage::carry_rig(w, &mut next);
                    *w = next;
                    Ok::<_, app_shell::WorkspaceError>(())
                });
            }
        }));
        use_effect(move || {
            let asked = shown();
            // The mixer is in the main window, beside the arrangement.
            if asked == Ui::Mixer {
                shown.set(to_ui(view));
                return;
            }
            if asked != to_ui(view) {
                navigator().replace(route(of_ui(asked)));
            }
        });
        let kinds = hosted::kinds(ui, super::page);
        rsx! {
            document::Title { "Studio — {title}" }
            div {
                class: "window",
                // ⌘⇧H (Ctrl+Shift+H): home to the launcher.
                onkeydown: move |e: KeyboardEvent| {
                    let m = e.modifiers();
                    let key = e.key().to_string();
                    if (m.meta() || m.ctrl()) && m.shift() && key.eq_ignore_ascii_case("h") {
                        e.prevent_default();
                        leave(app);
                    }
                    // ⌘⇧L: the launcher in a window of its own.
                    if (m.meta() || m.ctrl()) && m.shift() && key.eq_ignore_ascii_case("l") {
                        e.prevent_default();
                        hosted::open_launcher_window();
                    }
                },
                AppShell {
                    theme: use_hook(crate::shell::theme),
                    title: title.clone(),
                    layout,
                    kinds,
                    // The Stage's splits (its groups bare: no tab bars).
                    tabs: Tabs::On,
                    top_bar: rsx! {
                        TopBar { title: title.clone() }
                    },
                    accessory: rsx! {
                        studio_panels::shell::BottomBar {
                            view: shown,
                            mode,
                            transport: rsx! { Transport {} },
                        }
                    },
                    layers: rsx! {
                        ExtensionOverlays { ui }
                        SetBar { ui }
                    },
                }
            }
        }
    }

    /// Puts daw-view-kit's top bar in the shell's top bar's middle (all but
    /// the window's own controls).
    #[component]
    fn SetBar(ui: ExtensionUi) -> Element {
        // The center (between the title and the trailing items): the whole
        // middle (`BarSpan::Middle`) beside the tab tools panics
        // dioxus-vdom's placement on the next diff once tabs are on.
        use_top_bar_span("studio/setlist", BarSpan::Center, move || {
            rsx! {
                SessionTop {}
            }
        });
        let _ = ui;
        rsx! {}
    }

    /// daw-view-kit's top bar: the navigator's switch, the set's songs as
    /// tabs (a pick goes to that song, as the app's set does), Save and
    /// the checkpoints, and the engine's state at the right.
    #[component]
    fn SessionTop() -> Element {
        let app = use_app();
        rsx! {
            div { style: "flex:1 1 0px; min-width:0; height:100%; display:flex; flex-direction:column;",
                studio_panels::shell::TopBar {
                    lights: 0.0,
                    badges: rsx! { EngineBadge {} },
                    on_pick: move |i: usize| go_to(app, i),
                    on_color: move |(i, color): (usize, Option<String>)| {
                        engine_client::view_client::ask::recolor(i, color);
                    },
                }
            }
        }
    }

    /// The engine Studio plays through, as a word and a light.
    #[component]
    fn EngineBadge() -> Element {
        let app = use_app();
        let engine = (app.engine)();
        let (dot, word) = match &engine {
            crate::state::EngineStatus::Ready(_) if (app.engine_drives)() => ("ok", "Engine"),
            crate::state::EngineStatus::Ready(_) => ("idle", "Engine"),
            crate::state::EngineStatus::Linking => ("busy", "Starting"),
            crate::state::EngineStatus::Opening(_) => ("busy", "Loading"),
            crate::state::EngineStatus::Failed(_) => ("bad", "Timer"),
            crate::state::EngineStatus::Off => ("idle", "No engine"),
        };
        rsx! {
            span { class: "sbbadge {dot}", title: "{engine.line()}",
                i {}
                "{word}"
            }
        }
    }

    /// daw-view-kit's transport over the song on show (it reads the song
    /// it drives).
    #[component]
    fn Transport() -> Element {
        let song = studio_view_link::use_song();
        match song() {
            Some((key, session)) => rsx! {
                for key in std::iter::once(key) {
                    WithSong { key: "{key}", session: Song(session.clone()),
                        studio_panels::transport_bar::TransportBar { big: true }
                    }
                }
            },
            None => rsx! {},
        }
    }

    /// A song's session as a prop (keyed by its song, so never compared).
    #[derive(Clone)]
    struct Song(engine_client::song_model::StudioSession);

    impl PartialEq for Song {
        fn eq(&self, _: &Self) -> bool {
            true
        }
    }

    #[component]
    fn WithSong(session: Song, children: Element) -> Element {
        // The song as the views read it (daw-view's), beside Studio's session.
        use_context_provider(|| session.0.song());
        use_context_provider(|| session.0.clone());
        children
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_parse_by_slug() {
        assert_eq!(View::parse("daw"), Some(View::Arrangement));
        assert_eq!(View::parse("Mixer"), Some(View::Mixer));
        assert_eq!(View::parse("nope"), None);
        for v in View::ALL {
            assert_eq!(View::parse(v.slug()), Some(v));
        }
    }
}
