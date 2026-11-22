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
    AppShell, Destination, FormFactor, Icon as ShellIcon, PrimaryAction, SearchField, SheetId,
    SheetSpec, Sidebar, SidebarItem, SidebarSection, SidebarSwitcher, TopBar, icons, use_shell,
};
use dioxus::prelude::*;

use crate::data::{self, DAY};
use crate::icons::{I, Icon};
use crate::pieces::SongArt;
use crate::sheets::{MenuView, SheetBody};
use crate::state::{Sheet, use_app};
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
fn theme() -> Theme {
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
fn title_of(route: &Route) -> String {
    match route {
        Route::Home {} => "Home".into(),
        Route::Records {} | Route::Album { .. } | Route::TrackPage { .. } => "Records".into(),
        Route::Setlists {} | Route::Every {} => "Setlists".into(),
        Route::Series { id } => data::series(id).map_or("Setlists".into(), |s| s.name.into()),
        Route::SetPage { id } | Route::Chart { id } | Route::Lyrics { id } => data::set_title(id),
        Route::LibraryHome {} | Route::Library { .. } => "Library".into(),
        Route::SongPage { .. } | Route::SharedSong { .. } => "Song".into(),
        Route::Signal {} => "Signal".into(),
        Route::AllOrgs {} => "All orgs".into(),
        Route::People {} => "People".into(),
        Route::OrgSettings {} => "Organization".into(),
    }
}

#[component]
pub fn Shell() -> Element {
    let app = use_app();
    let route = use_route::<Route>();

    // The playheads move while playing.
    use_future(move || async move {
        let mut app = app;
        loop {
            crate::state::sleep_ms(1000).await;
            if app.np.peek().as_ref().is_some_and(|n| n.playing) {
                app.np.with_mut(|n| {
                    if let Some(n) = n {
                        n.pos = (n.pos + 0.004).min(1.0);
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

    let set = route.set_id().map(str::to_string);
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
        match (set_for_dest.as_deref(), d.as_str()) {
            (Some(id), "chart") => {
                nav.replace(Route::Chart { id: id.into() });
            }
            (Some(id), "lyrics") => {
                nav.replace(Route::Lyrics { id: id.into() });
            }
            (Some(_), "more") => app.open(Sheet::More),
            (Some(id), _) => {
                nav.replace(Route::set(id));
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
    let action = round_action(app, set.as_deref());
    let accessory = if song_view {
        Some(rsx! {
            Transport { set: set.clone().unwrap_or_default() }
        })
    } else if (app.np)().is_some() {
        Some(rsx! {
            NowPlayingBar {}
        })
    } else {
        None
    };

    rsx! {
        div { class: "window",
            AppShell {
                theme: use_hook(theme),
                title: title_of(&route),
                top_bar: rsx! {
                    StudioTopBar { title: title_of(&route) }
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
                    if let Some(id) = set.clone() {
                        crate::live_sync::SetRoom {
                            key: "{id}",
                            id,
                            view: crate::live_sync::view_of(&route),
                        }
                    }
                    main { class: "stage", Outlet::<Route> {} }
                    MenuView {}
                }
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
fn Content(class: String, children: Element) -> Element {
    let form = match use_shell().form() {
        FormFactor::Phone => "phone",
        FormFactor::Desktop => "desktop",
    };
    rsx! {
        div { class: "content {class}", "data-form": form, {children} }
    }
}

/// The round button: Open anything; in a set, its primary action.
fn round_action(app: crate::state::AppState, set: Option<&str>) -> Option<PrimaryAction> {
    let Some(id) = set.map(str::to_string) else {
        let mut a = PrimaryAction::new("Open anything", icons::SEARCH, move |()| {
            app.open(Sheet::Palette)
        });
        a.prominent = false;
        return Some(a);
    };
    if let Some(r) = data::mine(&id) {
        let say = format!("Opened {} in the {} rig", data::set_title(&id), r.name);
        return Some(PrimaryAction::new(
            format!("Open in {}", r.name),
            RIG,
            move |()| app.say(say.clone()),
        ));
    }
    Some(PrimaryAction::new("Play the set", PLAY, move |()| {
        let list = data::set_songs(&id);
        if let Some(first) = list.first().map(|s| s.title) {
            app.play(list, data::set_title(&id), first);
        }
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

/// The prototype's sheets, opened as the shell's: one at a time, closed
/// when the app closes it, and the app told when the person does.
#[component]
fn SheetBridge() -> Element {
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
fn ToastBridge() -> Element {
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
fn NowPlayingBar() -> Element {
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
                    span { class: "s1", "{x.artist.unwrap_or(&n.from)}" }
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
                        if let Some(n) = n && n.i + 1 < n.list.len() {
                            n.i += 1;
                            n.pos = 0.0;
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

/// In Chart and Lyrics: the click, the previous section, play, the next section, loop.
#[component]
fn Transport(set: String) -> Element {
    let app = use_app();
    let mut live = app.live;
    let l = live();
    let n = data::set_songs(&set).len();
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
    let shows_n: usize = data::FOLDERS.iter().map(|f| f.shows.len()).sum();
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
                div { class: "sideacct",
                    span { class: "avatar", style: "--h:{me_hue}", "{me_mark}" }
                    span { class: "who",
                        b { "{me_name}" }
                        span {
                            if who.is_empty() {
                                "Not signed in"
                            } else if me.is_some() {
                                "Signed in · you, in this org"
                            } else {
                                "Signed in"
                            }
                        }
                    }
                }
            },
            SidebarSection { title: "Studio",
                SidebarItem { label: "Home", icon: icons::HOME, selected: route == Route::Home {}, on_press: at(Route::Home {}) }
                SidebarItem { label: "Records", icon: DISC, badge: data::RECENT_WORK.len().to_string(), selected: tab == Tab::Records, on_press: at(Route::Records {}) }
                SidebarItem { label: "Setlists", icon: SETLIST, badge: (data::SERIES.len() + shows_n).to_string(), selected: in_set_pages, on_press: at(Route::Setlists {}) }
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
            SidebarSection { title: "Recurring",
                for sr in data::SERIES.iter() {
                    SidebarItem {
                        key: "{sr.id}",
                        label: sr.name,
                        icon: CALENDAR,
                        badge: sr.next().map(|n| if n.date.from_today() == 0 { "Today".to_string() } else { DAY[n.date.weekday()][..3].to_string() }).unwrap_or_default(),
                        selected: matches!(&route, Route::Series { id } if id == sr.id),
                        on_press: at(Route::Series { id: sr.id.into() }),
                    }
                }
            }
            SidebarSection { title: "Shows",
                for id in data::PINNED {
                    if let Some(st) = data::show(id) {
                        SidebarItem {
                            key: "{id}",
                            label: st.title,
                            icon: SETLIST,
                            badge: st.songs().len().to_string(),
                            selected: route.set_id() == Some(id),
                            on_press: at(Route::set(id)),
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
