//! The window, on app-shell: on a desktop its Spacedrive-style sidebar
//! (the org switcher, Studio's places, the org's people, the weekly
//! events and shows) and its top bar; on a phone the page with its own
//! bar, and at the foot the tab pill (in a set, the set's tabs), the
//! round button and the accessory over them (Now Playing, or in Chart
//! and Lyrics the transport). The sidebar is the phone's drawer. The
//! prototype's sheets open as app-shell's sheets (bottom sheets on a
//! phone, panels on a desktop), its toasts as app-shell's toasts.

use app_shell::Theme;
use app_shell::ui::{
    AppShell, BarPlace, Destination, FormFactor, Icon as ShellIcon, PrimaryAction, SearchField,
    SheetId, SheetSpec, Sidebar, SidebarItem, SidebarSection, SidebarSwitcher, TopBar, icons,
    use_shell, use_top_bar_item,
};
use dioxus::prelude::*;

use crate::data::{self, DAY, TODAY};
use crate::icons::{I, Icon};
use crate::pieces::SongArt;
use crate::sheets::{MenuView, SheetBody};
use crate::state::{SetRef, Sheet, use_app};
use crate::{Route, Tab};

// Lucide's paths, for the shell's icons (it draws path data only).
const DISC: ShellIcon = ShellIcon(&[
    "M12 2a10 10 0 1 0 0 20a10 10 0 1 0 0-20z",
    "M12 10a2 2 0 1 0 0 4a2 2 0 1 0 0-4z",
]);
const SETLIST: ShellIcon = ShellIcon(&[
    "M21 15V6",
    "M18.5 18a2.5 2.5 0 1 0 0-5a2.5 2.5 0 0 0 0 5z",
    "M12 12H3",
    "M16 6H3",
    "M12 18H3",
]);
const RIG: ShellIcon = ShellIcon(&[
    "M4 21v-7",
    "M4 10V3",
    "M12 21v-9",
    "M12 8V3",
    "M20 21v-5",
    "M20 12V3",
    "M1 14h6",
    "M9 8h6",
    "M17 16h6",
]);
const LIBRARY: ShellIcon = ShellIcon(&["m16 6 4 14", "M12 6v14", "M8 8v12", "M4 4v16"]);
const USERS: ShellIcon = ShellIcon(&[
    "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2",
    "M9 3a4 4 0 1 0 0 8a4 4 0 1 0 0-8z",
    "M22 21v-2a4 4 0 0 0-3-3.87",
    "M16 3.13a4 4 0 0 1 0 7.75",
]);
const CHART: ShellIcon = ShellIcon(&[
    "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z",
    "M14 2v6h6",
    "M10 18a2 2 0 1 0 0-4a2 2 0 0 0 0 4z",
    "M12 16v-6l3 1",
]);
const LYRICS: ShellIcon = ShellIcon(&["M4 7V4h16v3", "M9 20h6", "M12 4v16"]);
const PLAY: ShellIcon = ShellIcon(&["M6 3l14 9-14 9z"]);
const CALENDAR: ShellIcon = ShellIcon(&["M8 2v4", "M16 2v4", "M3 6h18v15H3z", "M3 10h18"]);
const SHARE: ShellIcon = ShellIcon(&[
    "M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8",
    "m16 6-4-4-4 4",
    "M12 2v13",
]);
const BUILDING: ShellIcon = ShellIcon(&[
    "M6 22V4a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v18z",
    "M6 12H4a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2h2",
    "M18 9h2a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-2",
    "M10 6h4",
    "M10 10h4",
    "M10 14h4",
    "M10 18h4",
]);

/// Studio's colours on the shell's variables: the prototype's true black,
/// a sidebar a shade apart, the teal accent (Drive's, so the apps match).
pub(crate) fn theme() -> Theme {
    Theme::dark()
        .set("--as-bg", "#000000")
        .set("--as-surface", "#0e0e11")
        .set("--as-raised", "#1c1c21")
        .set("--as-overlay", "rgba(0, 0, 0, 0.6)")
        .set("--as-border", "#2b2b31")
        .set("--as-text", "#f4f4f5")
        .set("--as-muted", "#a1a1aa")
        .set("--as-heading", "#71717a")
        .set("--as-accent", "#2dd4bf")
        .set("--as-accent-soft", "rgba(45, 212, 191, 0.22)")
        .set("--as-topbar-bg", "#000000")
        .set("--as-sidebar-bg", "rgba(14, 14, 17, 0.96)")
        .set("--as-selected", "rgba(255, 255, 255, 0.08)")
        .set("--as-selected-text", "#ffffff")
        .set("--as-search-bg", "rgba(255, 255, 255, 0.05)")
        .set(
            "--as-font",
            "\"Instrument Sans\", -apple-system, system-ui, sans-serif",
        )
        .set("--as-font-size", "14px")
        .set("--as-radius", "8px")
}

/// A page's title, for the desktop's top bar.
fn title_of(route: &Route, index: &crate::sets::Index) -> String {
    match route {
        Route::Home {} => "Home".into(),
        Route::Records {} | Route::Album { .. } | Route::TrackPage { .. } => "Records".into(),
        Route::Setlists {} | Route::Every {} => "Setlists".into(),
        Route::Series { id } => index.series_of(id).map_or("Setlists".into(), |s| s.name),
        Route::SetPage { id } | Route::Chart { id } | Route::Lyrics { id } => {
            index.face(id).map_or("Setlist".into(), |f| match f.day() {
                Some(d) if !f.series.is_empty() => format!("{} · {}", f.title, d.short()),
                _ => f.title,
            })
        }
        Route::OrgSet { owner, .. } => format!("Setlist · {}", crate::orgs::org_name(owner)),
        Route::DriveSet { path, .. } => path
            .last()
            .map_or("Setlist".into(), |n| crate::setfile::name_of(n)),
        Route::NowChart {} => "Chart".into(),
        Route::SessionPage { .. } => "Session".into(),
        Route::PrintSet { .. } => "Print".into(),
        Route::LibraryHome {} | Route::Library { .. } => "Library".into(),
        Route::SongPage { .. } | Route::SharedSong { .. } => "Song".into(),
        Route::Signal {} => "Signal".into(),
        Route::AllOrgs {} => "All orgs".into(),
        Route::People {} => "People".into(),
        Route::OrgSettings {} => "Organization".into(),
        Route::JoinRoom { .. } => "Room".into(),
    }
}

/// What runs whichever mode the window is in, the launcher or the
/// session: the playheads' timers, the player (natively the engine) and
/// the Action List. The two modes are its children, so playback and the
/// doors outlive a switch between them.
#[component]
pub fn Root() -> Element {
    let app = use_app();
    // The playheads move while playing.
    use_future(move || async move {
        let mut app = app;
        loop {
            crate::state::sleep_ms(1000).await;
            // In a browser the player's audio moves it (crate::player).
            // Natively the engine's transport moves it once it plays
            // (crate::playback); the timer until then, or without one.
            if !cfg!(feature = "web")
                && !*app.engine_drives.peek()
                && app.np.peek().as_ref().is_some_and(|n| n.playing)
            {
                // A second of the song's length.
                app.np.with_mut(|n| {
                    if let Some(n) = n {
                        let (_, len) = n.secs();
                        n.pos = (n.pos + 1.0 / len.max(1.0)).min(1.0);
                    }
                });
            }
            if app.review.peek().playing {
                app.review.with_mut(|r| r.pos = (r.pos + 0.006).min(1.0));
            }
        }
    });

    // A chart you lead moves a bar at a time (one you follow moves with its leader).
    use_future(move || async move {
        let mut since = 0;
        loop {
            crate::state::sleep_ms(100).await;
            if !app.live.peek().playing || app.sync.is_following() {
                since = 0;
                continue;
            }
            since += 100;
            if since >= app.bar_ms() {
                since = 0;
                app.step_bar();
            }
        }
    });

    // The launcher's own window plays nothing and answers no actions: the
    // main window does both.
    let main = !crate::in_launcher_window();
    rsx! {
        Outlet::<Route> {}
        if main {
            PlayerHost {}
            crate::actions::ActionHost {}
            UpdateNotice {}
        }
    }
}

/// The desktop's update check and its notice (`self_update`); nothing
/// elsewhere.
#[component]
fn UpdateNotice() -> Element {
    #[cfg(all(feature = "native", not(target_os = "ios")))]
    return rsx! { crate::self_update::UpdateNotice {} };
    #[cfg(not(all(feature = "native", not(target_os = "ios"))))]
    rsx! {}
}

#[component]
pub fn Shell() -> Element {
    let app = use_app();
    let route = use_route::<Route>();
    let index = crate::sets::use_index();
    let me = crate::team::use_me();
    let here = (app.here)();

    // The setlist files in Drive, in every org this person is in (a
    // computer's locations are its own space's, whichever org the set is
    // for): found once, for the Setlists page, Home and the sidebar's count.
    #[cfg(feature = "drive")]
    {
        let spaces = {
            let mut v = vec![here.clone()];
            for o in (app.orgs)() {
                if !v.contains(&o) {
                    v.push(o);
                }
            }
            v
        };
        // Drive's, found once; Studio's own folder's, read again every two
        // seconds natively (a file added or edited by hand shows up).
        let mut from_drive = use_signal(|| None::<Vec<crate::files::Hit>>);
        let mut on_disk = use_signal(Vec::<crate::files::Hit>::new);
        use_effect(use_reactive!(|spaces| {
            spawn(async move {
                let found = crate::drive_sets::files_in(&spaces).await;
                from_drive.set(Some(found));
            });
        }));
        use_future(move || async move {
            if cfg!(target_arch = "wasm32") {
                return;
            }
            loop {
                let found = crate::drive_sets::on_disk(&app.here.peek());
                if *on_disk.peek() != found {
                    on_disk.set(found);
                }
                crate::state::sleep_ms(2000).await;
            }
        });
        use_effect(move || {
            let (disk, drive) = (on_disk(), from_drive());
            let mut sets = app.drive_sets;
            // On disk at once; from Drive once it answers.
            if drive.is_none() && disk.is_empty() && cfg!(target_arch = "wasm32") {
                return;
            }
            sets.set(Some(crate::drive_sets::merge(
                &disk,
                &drive.unwrap_or_default(),
            )));
        });
    }

    // The set a page is of, in its org (Chart and Lyrics follow the set
    // last entered, in whichever org that was).
    let set: Option<SetRef> = route.set_id().map(|id| match &route {
        Route::OrgSet { owner, .. } => SetRef::new(owner, id),
        Route::Chart { .. } | Route::Lyrics { .. } if app.live_set.peek().id == id => {
            app.live_set.peek().clone()
        }
        _ => SetRef::new(&here, id),
    });
    let song_view = matches!(route, Route::Chart { .. } | Route::Lyrics { .. });
    let track_view = matches!(route, Route::TrackPage { .. });
    let mut cls = vec!["studio"];
    if song_view {
        cls.push("songview");
    }
    if track_view {
        cls.push("trackview");
    }
    let cls = cls.join(" ");

    // The foot: a set's own tabs inside a set, Studio's four elsewhere.
    let (destinations, destination) = match &set {
        Some(_) => (
            vec![
                Destination::new("setlist", "Setlist", SETLIST),
                Destination::new("chart", "Chart", CHART),
                Destination::new("lyrics", "Lyrics", LYRICS),
                Destination::new("more", "More", icons::MORE),
            ],
            Some(
                match route {
                    Route::Chart { .. } => "chart",
                    Route::Lyrics { .. } => "lyrics",
                    _ => "setlist",
                }
                .to_string(),
            ),
        ),
        None => (
            vec![
                Destination::new("home", "Home", icons::HOME),
                Destination::new("records", "Records", DISC),
                Destination::new("setlists", "Setlists", SETLIST),
                Destination::new("signal", "Signal", RIG),
            ],
            Some(
                match route.tab() {
                    Tab::Home => "home",
                    Tab::Records => "records",
                    Tab::Setlists => "setlists",
                    Tab::Signal => "signal",
                }
                .to_string(),
            ),
        ),
    };
    let set_for_dest = set.clone();
    let on_destination = move |d: String| {
        let nav = navigator();
        match (set_for_dest.as_ref(), d.as_str()) {
            (Some(s), "chart") => {
                nav.replace(Route::Chart { id: s.id.clone() });
            }
            (Some(s), "lyrics") => {
                nav.replace(Route::Lyrics { id: s.id.clone() });
            }
            (Some(_), "more") => app.open(Sheet::More),
            (Some(s), _) => {
                nav.replace(crate::pages::set_route(&s.owner, &s.id, &app.here.peek()));
            }
            (None, "records") => {
                nav.push(Route::Records {});
            }
            (None, "setlists") => {
                nav.push(Route::Setlists {});
            }
            (None, "signal") => {
                nav.push(Route::Signal {});
            }
            (None, _) => {
                nav.push(Route::Home {});
            }
        }
    };
    let mine = set.as_ref().and_then(|s| {
        let f = index.face(&s.id).filter(|_| s.owner == here)?;
        let me = me.as_ref()?;
        f.band.contains(me).then_some(f.title)
    });
    let action = round_action(app, set.is_some(), mine);
    let accessory = if song_view {
        Some(rsx! {
            Transport {}
        })
    } else if (app.np)().is_some() {
        Some(rsx! {
            NowPlayingBar {}
        })
    } else {
        None
    };

    // The right sidebar is Now Playing, while something plays.
    let playing = (app.np)().is_some_and(|n| n.song().is_some());
    let right = playing.then(|| rsx! { NowPlayingPanel {} });
    // The window's title again on the way home from a session (which
    // titled it "Studio — <set>").
    let window_title = if crate::in_launcher_window() {
        "Launcher"
    } else {
        "Studio"
    };

    rsx! {
        document::Title { "{window_title}" }
        div { class: "window",
            AppShell {
                theme: use_hook(theme),
                right_sidebar: right,
                title: title_of(&route, &index),
                top_bar: rsx! {
                    StudioTopBar { title: title_of(&route, &index) }
                },
                sidebar: rsx! {
                    StudioSidebar { route: route.clone() }
                },
                destinations,
                destination,
                on_destination,
                action,
                accessory,
                Content { class: cls,
                    if let Some(s) = set.clone() {
                        crate::pages::set::SetLive {
                            key: "{s.owner}/{s.id}",
                            set: s,
                            view: crate::live_sync::view_of(&route),
                        }
                    }
                    main { class: "stage", Outlet::<Route> {} }
                    MenuView {}
                }
                ChartButton {}
                SheetBridge {}
                ToastBridge {}
                crate::team::ShareMirror {}
            }
        }
    }
}

/// The content area, marked with the form it is drawn for (the pages'
/// own bars differ: a phone's leads with the drawer, a desktop's leaves
/// back and forward to the top bar).
#[component]
pub(crate) fn Content(class: String, children: Element) -> Element {
    let form = match use_shell().form() {
        FormFactor::Phone => "phone",
        FormFactor::Desktop => "desktop",
    };
    rsx! {
        div { class: "content {class}", "data-form": form, {children} }
    }
}

/// The round button: Open anything; in a set, its primary action (open it
/// in your rig when you play it, else play it).
fn round_action(
    app: crate::state::AppState,
    in_set: bool,
    mine: Option<String>,
) -> Option<PrimaryAction> {
    if !in_set {
        let mut a = PrimaryAction::new("Open anything", icons::SEARCH, move |()| {
            app.open(Sheet::Palette)
        });
        a.prominent = false;
        return Some(a);
    }
    if let Some(title) = mine {
        let r = data::loaded_rig().map_or("Guitar", |r| r.name);
        let say = format!("Opened {title} in the {r} rig");
        return Some(PrimaryAction::new(format!("Open in {r}"), RIG, move |()| {
            app.say(say.clone())
        }));
    }
    Some(PrimaryAction::new("Play the set", PLAY, move |()| {
        let titles: Vec<String> = app
            .live_list
            .peek()
            .iter()
            .map(|s| s.title.clone())
            .collect();
        app.play_titles(&titles, "This set".into(), None);
    }))
}

/// A desktop's top bar: back and forward, the page's title, Open
/// anything, and who else is here. A phone's pages have bars of their own.
#[component]
fn StudioTopBar(title: String) -> Element {
    let app = use_app();
    let shell = use_shell();
    if shell.form() == FormFactor::Phone {
        return rsx! {};
    }
    let nav = navigator();
    rsx! {
        TopBar {
            title,
            search: rsx! {
                div {
                    class: "tbsearch",
                    onclick: move |_| app.open(Sheet::Palette),
                    SearchField { placeholder: "Open anything", value: String::new(), on_input: move |_: String| app.open(Sheet::Palette) }
                }
            },
            actions: rsx! {
                crate::live_sync::Here {}
            },
            on_back: move |()| nav.go_back(),
            on_forward: move |()| nav.go_forward(),
            can_back: nav.can_go_back(),
            can_forward: nav.can_go_forward(),
        }
    }
}

/// While a set is loaded: the way back into its session, from the top
/// bar's trailing place.
#[component]
fn ChartButton() -> Element {
    let app = use_app();
    use_top_bar_item("studio/session", BarPlace::Trailing, 0, move || {
        if !(app.np)().is_some_and(|n| n.song().is_some()) {
            return rsx! {};
        }
        rsx! {
            button {
                class: "tbchart",
                title: "Back to the session playing",
                onclick: move |_| {
                    navigator().push(crate::session::route(crate::session::View::Arrangement));
                },
                Icon { name: I::Layers, size: 15 }
                span { "Session" }
            }
        }
    });
    rsx! {}
}

/// The right sidebar: the song playing, its sections (a tap plays from
/// one) and what comes after it.
#[component]
fn NowPlayingPanel() -> Element {
    let app = use_app();
    let Some(n) = (app.np)().filter(|n| n.song().is_some()) else {
        return rsx! {};
    };
    let song = n.song().map(|s| s.title).unwrap_or_default();
    let (now, len) = n.secs();
    let at = n.timeline.at(now).map(|s| s.name.clone());
    let sections = n.timeline.sections.clone();
    let next: Vec<String> = n
        .list
        .iter()
        .skip(n.i + 1)
        .take(6)
        .map(|s| s.title.to_string())
        .collect();
    rsx! {
        div { class: "rnp",
            div { class: "rnp-k", "Now playing" }
            div { class: "rnp-t", "{song}" }
            div { class: "rnp-s", "{n.from} · {crate::songs::clock(now)} / {crate::songs::clock(len)}" }
            if !sections.is_empty() {
                div { class: "rnp-k", "Sections" }
                div { class: "rnp-secs",
                    for s in sections {
                        button {
                            key: "{s.start}",
                            class: if at.as_deref() == Some(s.name.as_str()) { "rnp-sec on" } else { "rnp-sec" },
                            onclick: {
                                let start = s.start;
                                move |_| {
                                    let mut seek = app.seek;
                                    seek.set(Some(start));
                                }
                            },
                            span { "{s.name}" }
                            span { class: "rnp-at", "{crate::songs::clock(s.start)}" }
                        }
                    }
                }
            }
            if !next.is_empty() {
                div { class: "rnp-k", "Up next" }
                for (i, t) in next.iter().enumerate() {
                    div { key: "{i}", class: "rnp-next", "{t}" }
                }
            }
        }
    }
}

/// The prototype's sheets, opened as the shell's: one at a time, closed
/// when the app closes it, and the app told when the person does.
#[component]
pub(crate) fn SheetBridge() -> Element {
    let app = use_app();
    let shell = use_shell();
    let mut open = use_signal(|| None::<(Sheet, SheetId)>);
    use_effect(move || {
        let want = (app.sheet)();
        let have = open.peek().clone();
        if have.as_ref().map(|h| &h.0) == want.as_ref() {
            return;
        }
        if let Some((_, id)) = have {
            shell.close_sheet(id);
            open.set(None);
        }
        if let Some(sheet) = want {
            let big = matches!(sheet, Sheet::Palette | Sheet::Player | Sheet::Filters);
            let detent = if big {
                app_shell::gestures::Detent::Large
            } else {
                app_shell::gestures::Detent::Medium
            };
            let body = sheet.clone();
            let spec = SheetSpec::new(move |()| {
                rsx! {
                    SheetBody { sheet: body.clone() }
                }
            })
            .detents(
                detent,
                &[
                    app_shell::gestures::Detent::Medium,
                    app_shell::gestures::Detent::Large,
                ],
            )
            .on_close(move |()| {
                open.set(None);
                let mut s = app.sheet;
                s.set(None);
            });
            let id = shell.open_sheet(spec);
            open.set(Some((sheet, id)));
        }
    });
    rsx! {}
}

/// The app's toasts, as the shell's.
#[component]
pub(crate) fn ToastBridge() -> Element {
    let app = use_app();
    let shell = use_shell();
    use_effect(move || {
        if let Some((_, msg)) = (app.toast)() {
            shell.toast(msg);
        }
    });
    rsx! {}
}

/// Now Playing: above the tabs while something is queued; a tap opens the player.
#[component]
pub(crate) fn NowPlayingBar() -> Element {
    let app = use_app();
    let mut np = app.np;
    let Some(n) = np() else { return rsx! {} };
    let Some(x) = n.song().copied() else {
        return rsx! {};
    };
    let pct = n.pos * 100.0;
    rsx! {
        div { class: "np",
            button { class: "npmain", onclick: move |_| app.open(Sheet::Player),
                SongArt { song: x }
                span { class: "mid",
                    span { class: "t1", "{x.title}" }
                    span { class: "s1", "{now_line(&n)}" }
                }
            }
            button {
                class: "npb",
                aria_label: if n.playing { "Pause" } else { "Play" },
                onclick: move |_| np.with_mut(|n| if let Some(n) = n { n.playing = !n.playing }),
                Icon { name: if n.playing { I::Pause } else { I::Play }, size: 20, w: 2.0 }
            }
            button {
                class: "npb",
                aria_label: "Next",
                onclick: move |_| {
                    np.with_mut(|n| {
                        if let Some(n) = n {
                            n.next();
                        }
                    })
                },
                Icon { name: I::Skip, size: 20, w: 2.0 }
            }
            span { class: "npprog",
                i { style: "width:{pct}%" }
            }
        }
    }
}

/// Under the playing song's title: its section, or what the player is
/// doing, or its artist.
pub fn now_line(n: &crate::state::NowPlaying) -> String {
    use crate::state::Source;
    let artist = n
        .song()
        .and_then(|s| s.artist)
        .unwrap_or(&n.from)
        .to_string();
    match &n.source {
        Source::Finding => "Finding its reference…".into(),
        Source::Missing(why) => format!("Not playable: {why}"),
        _ => n
            .timeline
            .at(n.secs().0)
            .map_or(artist.clone(), |s| format!("{} · {artist}", s.name)),
    }
}

/// In Chart and Lyrics: the click, the previous section, play, the next section, loop.
#[component]
fn Transport() -> Element {
    let app = use_app();
    let mut live = app.live;
    let l = live();
    let n = (app.live_list)().len();
    let secs = data::SECS.len();
    rsx! {
        div { class: "transport",
            button {
                class: if l.click { "tp on" } else { "tp" },
                aria_label: "Click",
                aria_pressed: "{l.click}",
                onclick: move |_| live.with_mut(|l| l.click = !l.click),
                Icon { name: I::Metronome, size: 20, w: 2.0 }
            }
            button {
                class: "tp",
                aria_label: "Previous section",
                onclick: move |_| {
                    live.with_mut(|l| {
                        l.bar = 0;
                        if l.sec > 0 {
                            l.sec -= 1;
                        } else if l.song > 0 {
                            l.song -= 1;
                            l.sec = secs - 1;
                        }
                    })
                },
                Icon { name: I::ChevLeft, size: 20, w: 2.0 }
            }
            button {
                class: "tp big",
                aria_label: if l.playing { "Pause" } else { "Play" },
                onclick: move |_| live.with_mut(|l| l.playing = !l.playing),
                Icon { name: if l.playing { I::Pause } else { I::Play }, size: 22, w: 2.0 }
            }
            button {
                class: "tp",
                aria_label: "Next section",
                onclick: move |_| {
                    live.with_mut(|l| {
                        l.bar = 0;
                        if l.sec + 1 < secs {
                            l.sec += 1;
                        } else if l.song + 1 < n {
                            l.song += 1;
                            l.sec = 0;
                        }
                    })
                },
                Icon { name: I::ChevRight, size: 20, w: 2.0 }
            }
            button {
                class: if l.looping { "tp on" } else { "tp" },
                aria_label: if l.looping { "Looping the section" } else { "Loop the section" },
                aria_pressed: "{l.looping}",
                onclick: move |_| live.with_mut(|l| l.looping = !l.looping),
                Icon { name: I::Loop, size: 20, w: 2.0 }
            }
        }
    }
}

/// Who this is, at the sidebar's foot: the mark alone on the rail (an
/// iPad's folded sidebar has no room for the words).
#[component]
fn SideAccount(mark: String, hue: u32, name: String, line: String) -> Element {
    let rail = app_shell::ui::use_sidebar_mode() == app_shell::ui::SidebarMode::Rail;
    rsx! {
        div { class: "sideacct", title: if rail { "{name} · {line}" } else { "" },
            span { class: "avatar", style: "--h:{hue}", "{mark}" }
            if !rail {
                span { class: "who",
                    b { "{name}" }
                    span { "{line}" }
                }
            }
        }
    }
}

/// The sidebar (a phone's drawer): the org switcher, Studio's places,
/// the org's people and settings, the weekly events, the shows, the
/// loaded rig, and who is signed in.
#[component]
fn StudioSidebar(route: Route) -> Element {
    let app = use_app();
    let org = (app.org)();
    let here = (app.here)();
    let orgs = (app.orgs)();
    let who = (app.who)();
    let team = crate::team::use_team();
    let lib = crate::songs::use_library();
    let people = team.people();
    let me = team.me(&who);
    let tab = route.tab();
    let at = |r: Route| {
        move |()| {
            navigator().push(r.clone());
        }
    };
    let index = crate::sets::use_index();
    let faces = index.faces();
    let series = index.series();
    let mut coming: Vec<crate::setlist::Face> = faces
        .iter()
        .filter(|f| f.series.is_empty() && f.upcoming())
        .cloned()
        .collect();
    coming.sort_by(|a, b| a.date.cmp(&b.date));
    coming.truncate(4);
    // The org's sets, and the setlist files in its Drive.
    let upcoming_n =
        faces.iter().filter(|f| f.upcoming()).count() + (app.drive_sets)().map_or(0, |v| v.len());
    let (title, detail) = match &org {
        Some(_) => (team.name(), format!("Studio · {} orgs", orgs.len())),
        None => ("All orgs".to_string(), format!("{} orgs", orgs.len())),
    };
    let _ = here;
    let me_hue = me.as_ref().map_or(210, |m| m.hue);
    let me_mark = me.as_ref().map_or("·".to_string(), |m| m.initials());
    let me_name = match &me {
        Some(m) => m.name.clone(),
        None if who.is_empty() => "This browser".to_string(),
        None => who.clone(),
    };
    let in_set_pages = matches!(
        route,
        Route::Setlists {}
            | Route::Every {}
            | Route::Series { .. }
            | Route::SetPage { .. }
            | Route::Chart { .. }
            | Route::Lyrics { .. }
            | Route::OrgSet { .. }
            | Route::PrintSet { .. }
    );
    let loaded = data::loaded_rig();
    rsx! {
        Sidebar {
            header: rsx! {
                SidebarSwitcher {
                    title,
                    detail,
                    on_press: move |()| app.open(Sheet::Orgs),
                }
            },
            footer: rsx! {
                SideAccount {
                    mark: me_mark,
                    hue: me_hue,
                    name: me_name,
                    line: if who.is_empty() {
                        "Not signed in"
                    } else if me.is_some() {
                        "Signed in · you, in this org"
                    } else {
                        "Signed in"
                    },
                }
            },
            SidebarSection { title: "Studio",
                SidebarItem { label: "Home", icon: icons::HOME, selected: route == Route::Home {}, on_press: at(Route::Home {}) }
                SidebarItem { label: "Records", icon: DISC, badge: data::RECENT_WORK.len().to_string(), selected: tab == Tab::Records, on_press: at(Route::Records {}) }
                SidebarItem { label: "Setlists", icon: SETLIST, badge: upcoming_n.to_string(), selected: in_set_pages, on_press: at(Route::Setlists {}) }
                SidebarItem { label: "Library", icon: LIBRARY, badge: lib.entries().len().to_string(), selected: matches!(route, Route::LibraryHome {} | Route::Library { .. } | Route::SongPage { .. } | Route::SharedSong { .. }), on_press: at(Route::LibraryHome {}) }
                SidebarItem { label: "Signal", icon: RIG, badge: data::RIGS.len().to_string(), selected: tab == Tab::Signal, on_press: at(Route::Signal {}) }
            }
            SidebarSection { title: "Organization",
                SidebarItem { label: "People", icon: USERS, badge: people.len().to_string(), selected: route == Route::People {}, on_press: at(Route::People {}) }
                SidebarItem { label: "Shared songs", icon: SHARE, selected: matches!(route, Route::Library { ref tag } if tag == "shared"), on_press: at(Route::Library { tag: "shared".into() }) }
                SidebarItem { label: "Settings", icon: icons::SETTINGS, selected: route == Route::OrgSettings {}, on_press: at(Route::OrgSettings {}) }
                SidebarItem { label: "All orgs", icon: BUILDING, badge: orgs.len().to_string(), selected: route == Route::AllOrgs {}, on_press: move |()| {
                    let mut o = app.org;
                    o.set(None);
                    navigator().push(Route::AllOrgs {});
                } }
            }
            if !series.is_empty() {
                SidebarSection { title: "Recurring",
                    for sr in series {
                        SidebarItem {
                            key: "{sr.id}",
                            label: sr.name.clone(),
                            icon: CALENDAR,
                            badge: sr.next_from(TODAY).map(|d| if d.from_today() == 0 { "Today".to_string() } else { DAY[d.weekday()][..3].to_string() }).unwrap_or_default(),
                            selected: matches!(&route, Route::Series { id } if *id == sr.id),
                            on_press: at(Route::Series { id: sr.id.clone() }),
                        }
                    }
                }
            }
            if !coming.is_empty() {
                SidebarSection { title: "Coming up",
                    for f in coming {
                        SidebarItem {
                            key: "{f.id}",
                            label: f.title.clone(),
                            icon: SETLIST,
                            badge: f.day().map(|d| d.short()).unwrap_or_default(),
                            selected: route.set_id() == Some(f.id.as_str()),
                            on_press: at(Route::SetPage { id: f.id.clone() }),
                        }
                    }
                }
            }
            if let Some(r) = loaded {
                SidebarSection { title: "Loaded rig",
                    SidebarItem { label: r.name, icon: RIG, badge: r.preset, on_press: at(Route::Signal {}) }
                }
            }
        }
    }
}

/// The player's audio (a browser's; other builds play nothing yet).
#[component]
fn PlayerHost() -> Element {
    // In a browser: the engine plays each song from its session folder,
    // the reference player whatever it does not (no folder, or one that
    // did not open).
    #[cfg(feature = "web")]
    {
        use_context_provider(|| Signal::new(crate::engine_web::WebPlay::default()));
        rsx! {
            crate::engine_web::EnginePlayer {}
            crate::player::Player {}
            // A room the page is in: it lasts while its person moves
            // between pages.
            crate::room_web::RoomMember {}
        }
    }
    // Natively: no player yet (the engine will play); the sets and their
    // files kept in step, and the playing song's sections from its session.
    #[cfg(all(feature = "drive", not(feature = "web")))]
    {
        rsx! {
            crate::set_files::Sync {}
            crate::set_files::Sections {}
            EngineHostNative {}
        }
    }
    #[cfg(not(any(feature = "web", feature = "drive")))]
    {
        rsx! {}
    }
}

/// The engine, in the native build (none in a build without its doors).
#[cfg(all(feature = "drive", not(feature = "web")))]
#[component]
fn EngineHostNative() -> Element {
    #[cfg(feature = "native")]
    {
        rsx! {
            crate::playback::EngineHost {}
        }
    }
    #[cfg(not(feature = "native"))]
    {
        rsx! {}
    }
}
