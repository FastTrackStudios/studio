//! The phone shell: the page in the middle; at the foot the Now Playing
//! bar (or a set's transport) over the tab pill and the round button; the
//! sidebar from the left edge; menus, sheets and the toast over it all.

use dioxus::prelude::*;

use crate::data::{self, DAY};
use crate::icons::{I, Icon};
use crate::pieces::{Cover, SongArt};
use crate::sheets::{MenuView, SheetView};
use crate::state::{use_app, AppState, Sheet};
use crate::{Route, Tab};

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

    // The toast goes after a moment.
    let toast = app.toast;
    use_effect(move || {
        if let Some((n, _)) = toast() {
            spawn(async move {
                crate::state::sleep_ms(1600).await;
                let mut toast = toast;
                if toast.peek().as_ref().is_some_and(|t| t.0 == n) {
                    toast.set(None);
                }
            });
        }
    });

    let set = route.set_id().map(str::to_string);
    let song_view = matches!(route, Route::Chart { .. } | Route::Lyrics { .. });
    let track_view = matches!(route, Route::TrackPage { .. });
    let rig = set.as_deref().and_then(data::mine);
    let side = (app.side)();
    let mut cls = vec!["phone"];
    if side {
        cls.push("side-open");
    }
    if song_view {
        cls.push("songview");
        if rig.is_none() {
            cls.push("noround");
        }
    }
    if track_view {
        cls.push("trackview");
    }
    let cls = cls.join(" ");

    // A swipe in from the left edge brings the sidebar; a swipe left puts it away.
    let mut swipe = use_signal(|| None::<(f64, f64)>);

    rsx! {
        div {
            class: "{cls}",
            ontouchstart: move |e: TouchEvent| {
                if let Some(t) = e.touches().first() {
                    let p = t.client_coordinates();
                    swipe.set((side || p.x < 24.0).then_some((p.x, p.y)));
                }
            },
            ontouchmove: move |e: TouchEvent| {
                let Some((sx, sy)) = swipe() else { return };
                let Some(t) = e.touches().first().map(|t| t.client_coordinates()) else { return };
                let dx = t.x - sx;
                if (t.y - sy).abs() > dx.abs() {
                    swipe.set(None);
                    return;
                }
                let mut s = app.side;
                if !side && dx > 50.0 {
                    s.set(true);
                    swipe.set(None);
                } else if side && dx < -50.0 {
                    s.set(false);
                    swipe.set(None);
                }
            },
            if let Some(id) = set.clone() {
                crate::live_sync::SetRoom {
                    key: "{id}",
                    id,
                    view: crate::live_sync::view_of(&route),
                }
            }
            main { class: "stage", Outlet::<Route> {} }
            footer { class: "foot",
                if song_view {
                    Transport { set: set.clone().unwrap_or_default() }
                } else {
                    NowPlayingBar {}
                }
                div { class: "tabrow",
                    if let Some(id) = set.clone() {
                        SetTabs { id, route: route.clone() }
                    } else {
                        Tabs { current: route.tab() }
                    }
                    Round { set: set.clone() }
                }
            }
            div {
                class: "scrim",
                onclick: move |_| {
                    let mut s = app.side;
                    s.set(false);
                },
            }
            aside { class: "drawer", aria_label: "Sidebar", Drawer { current: route.tab() } }
            MenuView {}
            SheetView {}
            Toast {}
        }
    }
}

/// The four tabs, each labelled.
#[component]
fn Tabs(current: Tab) -> Element {
    let tabs = [
        (Tab::Home, "Home", I::Home, Route::Home {}),
        (Tab::Records, "Records", I::Disc, Route::Records {}),
        (Tab::Setlists, "Setlists", I::Setlist, Route::Setlists {}),
        (Tab::Signal, "Signal", I::Rig, Route::Signal {}),
    ];
    rsx! {
        nav { class: "tabs", aria_label: "Sections",
            for (tab , label , icon , to) in tabs {
                button {
                    key: "{label}",
                    class: "tab fav",
                    aria_label: "{label}",
                    aria_current: if tab == current { "page" } else { "false" },
                    onclick: move |_| {
                        navigator().push(to.clone());
                    },
                    Icon { name: icon, size: 22 }
                    span { "{label}" }
                }
            }
        }
    }
}

/// In a set: its own tabs, Setlist, Chart, Lyrics and More.
#[component]
fn SetTabs(id: String, route: Route) -> Element {
    let app = use_app();
    let cur = match route {
        Route::Chart { .. } => "chart",
        Route::Lyrics { .. } => "lyrics",
        _ => "setlist",
    };
    let tabs = [
        ("setlist", "Setlist", I::Setlist),
        ("chart", "Chart", I::Chart),
        ("lyrics", "Lyrics", I::Type),
        ("more", "More", I::More),
    ];
    rsx! {
        nav { class: "tabs", aria_label: "This set",
            for (v , label , icon) in tabs {
                button {
                    key: "{v}",
                    class: "tab fav",
                    aria_label: "{label}",
                    aria_current: if v == cur { "page" } else { "false" },
                    onclick: {
                        let id = id.clone();
                        move |_| {
                            let nav = navigator();
                            match v {
                                "chart" => {
                                    nav.replace(Route::Chart { id: id.clone() });
                                }
                                "lyrics" => {
                                    nav.replace(Route::Lyrics { id: id.clone() });
                                }
                                "more" => app.open(Sheet::More),
                                _ => {
                                    nav.replace(Route::set(&id));
                                }
                            }
                        }
                    },
                    Icon { name: icon, size: 22 }
                    span { "{label}" }
                }
            }
        }
    }
}

/// The round button: Open anything; in a set, its primary action.
#[component]
fn Round(set: Option<String>) -> Element {
    let app = use_app();
    let Some(id) = set else {
        return rsx! {
            button {
                class: "round",
                aria_label: "Open anything",
                onclick: move |_| app.open(Sheet::Palette),
                Icon { name: I::Search, size: 22, w: 1.9 }
            }
        };
    };
    if let Some(r) = data::mine(&id) {
        let label = format!("Open in {}", r.name);
        let say = format!("Opened {} in the {} rig", data::set_title(&id), r.name);
        return rsx! {
            button {
                class: "round prim",
                style: "--h:{r.hue}",
                "data-rig": "true",
                aria_label: "{label}",
                onclick: move |_| app.say(say.clone()),
                Icon { name: r.icon, size: 22, w: 1.9 }
            }
        };
    }
    rsx! {
        button {
            class: "round prim",
            aria_label: "Play the set",
            onclick: move |_| {
                let list = data::set_songs(&id);
                if let Some(first) = list.first().map(|s| s.title) {
                    app.play(list, data::set_title(&id), first);
                }
            },
            Icon { name: I::Play, size: 22, w: 1.9 }
        }
    }
}

/// Now Playing: above the tabs while something is queued; a tap opens the player.
#[component]
fn NowPlayingBar() -> Element {
    let app = use_app();
    let mut np = app.np;
    let Some(n) = np() else { return rsx! {} };
    let Some(x) = n.song().copied() else { return rsx! {} };
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

/// The sidebar: the org switcher, the tabs, the weekly events, the shows,
/// the loaded rig, settings and the account.
#[component]
fn Drawer(current: Tab) -> Element {
    let app = use_app();
    let org = (app.org)();
    let go = move |to: Route| {
        let mut s = app.side;
        s.set(false);
        navigator().push(to);
    };
    let shows_n: usize = data::FOLDERS.iter().map(|f| f.shows.len()).sum();
    let tabs = [
        (Tab::Home, "Home", I::Home, Route::Home {}, String::new()),
        (Tab::Records, "Records", I::Disc, Route::Records {}, data::RECENT_WORK.len().to_string()),
        (Tab::Setlists, "Setlists", I::Setlist, Route::Setlists {}, (data::SERIES.len() + shows_n).to_string()),
        (Tab::Signal, "Signal", I::Rig, Route::Signal {}, data::RIGS.len().to_string()),
    ];
    let loaded = data::loaded_rig();
    rsx! {
        div { class: "scroll",
            button {
                class: "switcher",
                onclick: move |_| {
                    let mut s = app.side;
                    s.set(false);
                    app.open(Sheet::Orgs);
                },
                if let Some(o) = org {
                    span {
                        class: "mark",
                        style: "background:oklch(.45 .1 {data::org_hue(o)})",
                        "{data::org_name(o).chars().next().unwrap_or('?')}"
                    }
                } else {
                    span { class: "mark",
                        Icon { name: I::Layers, size: 16 }
                    }
                }
                span { class: "nm",
                    b { "{org.map_or(\"All organizations\", data::org_name)}" }
                    span {
                        if org.is_some() {
                            "Organization"
                        } else {
                            "{data::ORGS.len()} organizations"
                        }
                    }
                }
                span { style: "color:var(--ink-3)",
                    Icon { name: I::Updown, size: 16 }
                }
            }
            div { class: "sidetabs",
                for (tab , label , icon , to , n) in tabs {
                    button {
                        key: "{label}",
                        class: "sidetab",
                        aria_current: if tab == current { "page" } else { "false" },
                        onclick: move |_| go(to.clone()),
                        Icon { name: icon, size: 19 }
                        "{label}"
                        span { class: "n", "{n}" }
                    }
                }
                button {
                    class: "sidetab",
                    onclick: move |_| go(Route::LibraryHome {}),
                    Icon { name: I::Library, size: 19 }
                    "Library"
                    span { class: "n", "{data::LIB.len()}" }
                }
            }
            div { class: "sidehead", "Recurring" }
            for sr in data::SERIES.iter() {
                button {
                    key: "{sr.id}",
                    class: "siderow",
                    onclick: move |_| go(Route::Series { id: sr.id.into() }),
                    if let Some(l) = data::logo(sr.id) {
                        img { class: "slogo sm", src: l, alt: "" }
                    }
                    span { class: "lbl", "{sr.name}" }
                    span { class: "nt",
                        {
                            sr.next()
                                .map(|n| {
                                    if n.date.from_today() == 0 {
                                        "Today".to_string()
                                    } else {
                                        DAY[n.date.weekday()][..3].to_string()
                                    }
                                })
                                .unwrap_or_default()
                        }
                    }
                }
            }
            div { class: "sidehead", "Shows" }
            for id in data::PINNED {
                if let Some(st) = data::show(id) {
                    button {
                        key: "{id}",
                        class: "siderow",
                        onclick: move |_| go(Route::set(id)),
                        if let Some(a) = data::art(id) {
                            img { class: "slogo sm", src: a, alt: "" }
                        } else {
                            span { class: "slogo sm",
                                Cover { title: st.title, size: 24, hue: st.hue }
                            }
                        }
                        span { class: "lbl", "{st.title}" }
                        span { class: "nt", "{st.songs().len()}" }
                    }
                }
            }
            if let Some(r) = loaded {
                div { class: "sidehead", "Loaded rig" }
                button {
                    class: "siderow",
                    onclick: move |_| go(Route::Signal {}),
                    span { class: "ic",
                        span { class: "live" }
                    }
                    span { class: "lbl", "{r.name}" }
                    span { class: "nt", "{r.preset}" }
                }
            }
        }
        div { class: "footrows",
            button { onclick: move |_| app.say("Settings"),
                Icon { name: I::Settings, size: 18 }
                "Settings"
            }
        }
        div { class: "account",
            button { onclick: move |_| app.say("Account"),
                span { class: "avatar", "CW" }
                span { class: "who",
                    b { "Cody Wright" }
                    span { "Signed in · 2 devices" }
                }
                span { style: "color:var(--ink-3)",
                    Icon { name: I::Updown, size: 15 }
                }
            }
        }
    }
}

#[component]
fn Toast() -> Element {
    let app: AppState = use_app();
    let t = (app.toast)();
    let (cls, msg) = match &t {
        Some((_, m)) => ("toast show", m.clone()),
        None => ("toast", String::new()),
    };
    rsx! {
        div { class: "{cls}", role: "status", aria_live: "polite", "{msg}" }
    }
}
