//! What opens over the page: the ⋯ menus from its bar, and the sheets —
//! open anything, the player, a song's key and arrangement in a set, More,
//! booking a rehearsal, the library's filters, the org, a status.

use dioxus::prelude::*;

use crate::data::{self, dur, Song, DAY, FIFTHS, MON, TODAY};
use crate::icons::{I, Icon};
use crate::pieces::{Energy, KeyC, SongArt};
use crate::state::{use_app, AppState, Booking, Menu, Override, Sheet};
use crate::Route;

/// A row of a menu.
enum Row {
    Head(&'static str),
    Div,
    /// Label, icon, note, what it says when tapped, red.
    Item(String, I, String, String, bool),
    /// Label, checked, what it does.
    Check(String, bool, Act),
}

/// What a menu row does beyond saying so.
#[derive(Clone, Copy)]
enum Act {
    Folder(Option<&'static str>),
}

fn item(label: impl Into<String>, icon: I) -> Row {
    let l = label.into();
    Row::Item(l.clone(), icon, String::new(), l, false)
}

fn menu_rows(app: AppState, m: Menu) -> Vec<Row> {
    let folder = *app.folder.peek();
    match m {
        Menu::NewSet => {
            let mut v = vec![Row::Head("Next week of")];
            for sr in data::SERIES.iter() {
                let nx = data::OCC
                    .iter()
                    .find(|o| o.series.id == sr.id && o.date.from_today() >= 0 && o.songs.is_empty());
                let note = nx.map(|o| o.date.short()).unwrap_or_default();
                let says = match nx {
                    Some(o) => format!("New {} set for {}", sr.name, o.date.short()),
                    None => format!("New {} set", sr.name),
                };
                v.push(Row::Item(sr.name.into(), I::Calendar, note, says, false));
            }
            v.push(Row::Div);
            v.push(Row::Item("A show".into(), I::Plus, "named, one date".into(), "New show".into(), false));
            v.push(Row::Item("A new weekly event".into(), I::Loop, String::new(), "New weekly event".into(), false));
            v
        }
        Menu::SetMenu => {
            let mut v = vec![
                Row::Head("Show"),
                Row::Check("All folders".into(), folder.is_none(), Act::Folder(None)),
            ];
            for f in data::FOLDERS.iter() {
                v.push(Row::Check(f.name.into(), folder == Some(f.tag), Act::Folder(Some(f.tag))));
            }
            v.push(Row::Div);
            v.push(item("Every setlist", I::Calendar));
            v.push(item("Library", I::Library));
            v
        }
        Menu::Series => vec![
            item("Plan a set", I::Plus),
            item("Book a rehearsal", I::Mic),
            item("Change the days", I::Settings),
            Row::Item("Songs a set".into(), I::Setlist, "usual count".into(), "Songs a set".into(), false),
            Row::Div,
            item("Pin to the sidebar", I::Pin),
            item("Share…", I::Share),
        ],
        Menu::Occ => vec![
            item("Book a rehearsal", I::Mic),
            item("Copy last week's set", I::Loop),
            Row::Div,
            item("Share…", I::Share),
            item("Show in the vault", I::Folder),
        ],
        Menu::Project => vec![
            item("Open in Session", I::Open),
            item("Rehearse", I::Mic),
            item("Pin to the sidebar", I::Pin),
            Row::Div,
            item("Share…", I::Share),
            item("Show in the vault", I::Folder),
            Row::Div,
            Row::Item("Remove from recent".into(), I::Trash, String::new(), "Remove from recent".into(), true),
        ],
        Menu::Mine => {
            let mut v = vec![Row::Head("Open in your rig")];
            for o in data::my_sets() {
                if let Some(r) = o.mine() {
                    v.push(Row::Item(
                        o.name(),
                        r.icon,
                        format!("{} · {}", o.date.when(), r.name),
                        format!("Opened {}, {} in the {} rig", o.name(), o.date.short(), r.name),
                        false,
                    ));
                }
            }
            v.push(Row::Div);
            v.push(item("Tuner", I::Tuner));
            v
        }
    }
}

/// The open ⋯ menu, under the bar on the right; a tap outside closes it.
#[component]
pub fn MenuView() -> Element {
    let app = use_app();
    let mut menu = app.menu;
    let route = use_route::<Route>();
    let Some(m) = menu() else { return rsx! {} };
    let rows = menu_rows(app, m);
    rsx! {
        div { class: "menu-catch", onclick: move |_| menu.set(None) }
        div { class: "menu",
            for (k , row) in rows.into_iter().enumerate() {
                match row {
                    Row::Head(h) => rsx! {
                        div { key: "{k}", class: "mh", "{h}" }
                    },
                    Row::Div => rsx! {
                        div { key: "{k}", class: "md" }
                    },
                    Row::Check(label, on, act) => rsx! {
                        button {
                            key: "{k}",
                            class: "mi",
                            onclick: move |_| {
                                menu.set(None);
                                match act {
                                    Act::Folder(f) => {
                                        let mut fo = app.folder;
                                        fo.set(f);
                                    }
                                }
                            },
                            span { class: "ic" }
                            "{label}"
                            if on {
                                span { class: "ck",
                                    Icon { name: I::Check, size: 17, w: 2.2 }
                                }
                            }
                        }
                    },
                    Row::Item(label, icon, note, says, red) => {
                        let route = route.clone();
                        rsx! {
                            button {
                                key: "{k}",
                                class: "mi",
                                style: if red { "color:#f87171" },
                                onclick: move |_| {
                                    menu.set(None);
                                    match says.as_str() {
                                        "Book a rehearsal" => crate::pages::setlists::book_here(app, &route),
                                        "Every setlist" => {
                                            navigator().push(Route::Every {});
                                        }
                                        "Library" => {
                                            navigator().push(Route::LibraryHome {});
                                        }
                                        s => app.say(s),
                                    }
                                },
                                span { class: "ic", style: if red { "color:#f87171" },
                                    Icon { name: icon, size: 18 }
                                }
                                "{label}"
                                if !note.is_empty() {
                                    span { class: "nt", "{note}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The open sheet. A tap on the dark around it closes it.
#[component]
pub fn SheetView() -> Element {
    let app = use_app();
    let Some(sheet) = (app.sheet)() else { return rsx! {} };
    let bottom = !matches!(sheet, Sheet::Palette);
    rsx! {
        div {
            class: if bottom { "sheet bottom" } else { "sheet" },
            onclick: move |_| app.close(),
            match sheet {
                Sheet::Palette => rsx! {
                    Palette {}
                },
                Sheet::Player => rsx! {
                    Player {}
                },
                Sheet::Key(set, i) => rsx! {
                    KeySheet { set, i }
                },
                Sheet::Arr(set, i) => rsx! {
                    ArrSheet { set, i }
                },
                Sheet::More => rsx! {
                    MoreSheet {}
                },
                Sheet::Book => rsx! {
                    BookSheet {}
                },
                Sheet::Filters => rsx! {
                    FilterSheet {}
                },
                Sheet::Orgs => rsx! {
                    OrgSheet {}
                },
                Sheet::Status(target) => rsx! {
                    StatusSheet { target }
                },
            }
        }
    }
}

/// A bottom sheet's card: taps inside stay inside.
#[component]
fn BSheet(label: String, #[props(default)] class: Option<&'static str>, children: Element) -> Element {
    let cls = match class {
        Some(c) => format!("bsheet {c}"),
        None => "bsheet".into(),
    };
    rsx! {
        div {
            class: "{cls}",
            role: "dialog",
            aria_label: "{label}",
            onclick: move |e| e.stop_propagation(),
            div { class: "grab" }
            {children}
        }
    }
}

/// Something the palette can open.
#[derive(Clone, PartialEq)]
struct Hit {
    t: String,
    k: &'static str,
    open: Option<Route>,
    says: String,
    key: Option<&'static str>,
    icon: I,
}

fn palette_items() -> Vec<Hit> {
    let mut v = Vec::new();
    for st in data::SHOWS.iter() {
        v.push(Hit { t: st.title.into(), k: "Set", open: Some(Route::set(st.id)), says: String::new(), key: None, icon: I::Setlist });
    }
    for sr in data::SERIES.iter() {
        v.push(Hit { t: sr.name.into(), k: "Weekly", open: Some(Route::Series { id: sr.id.into() }), says: String::new(), key: None, icon: I::Calendar });
    }
    for a in data::ALBUMS.iter() {
        v.push(Hit { t: a.title.into(), k: "Album", open: Some(Route::Album { id: a.id.into() }), says: String::new(), key: None, icon: I::Disc });
    }
    let mut seen: Vec<&str> = Vec::new();
    for st in data::SHOWS.iter() {
        for s in st.songs() {
            if !seen.contains(&s.title) {
                seen.push(s.title);
                v.push(Hit { t: s.title.into(), k: "Song", open: None, says: format!("Opened {}", s.title), key: Some(s.key.unwrap_or("?")), icon: I::Audio });
            }
        }
    }
    for r in data::RIGS.iter() {
        v.push(Hit { t: format!("{} rig", r.name), k: "Rig", open: Some(Route::Signal {}), says: String::new(), key: None, icon: I::Rig });
    }
    for a in ["New project", "Scan for projects", "Tuner", "Audio settings"] {
        v.push(Hit { t: a.into(), k: "Action", open: None, says: a.into(), key: None, icon: I::Command });
    }
    v
}

/// Open anything: a search from the top over everything there is.
#[component]
fn Palette() -> Element {
    let app = use_app();
    let mut q = use_signal(String::new);
    let all = use_hook(palette_items);
    let needle = q().trim().to_lowercase();
    let list: Vec<Hit> = if needle.is_empty() {
        all.iter().take(6).cloned().collect()
    } else {
        all.iter().filter(|h| h.t.to_lowercase().contains(&needle)).take(12).cloned().collect()
    };
    rsx! {
        div { class: "panel", onclick: move |e| e.stop_propagation(),
            label { class: "field",
                Icon { name: I::Search, size: 18 }
                input {
                    placeholder: "Projects, songs, rigs, actions",
                    aria_label: "Open anything",
                    value: "{q}",
                    onmounted: move |e| async move {
                        let _ = e.data().set_focus(true).await;
                    },
                    oninput: move |e| q.set(e.value()),
                }
                button { class: "cancel", onclick: move |_| app.close(), "Cancel" }
            }
            div { class: "results",
                if needle.is_empty() {
                    div { class: "mh", "Recent" }
                }
                if list.is_empty() {
                    div { class: "mh", "Nothing called “{needle}”" }
                }
                for h in list {
                    button {
                        key: "{h.k}{h.t}",
                        class: "res",
                        onclick: {
                            let h = h.clone();
                            move |_| {
                                app.close();
                                match &h.open {
                                    Some(r) => {
                                        navigator().push(r.clone());
                                    }
                                    None => app.say(h.says.clone()),
                                }
                            }
                        },
                        if let Some(k) = h.key {
                            span {
                                class: if k == "?" { "tile nokey" } else { "tile key" },
                                style: "--h:{data::hue(k).unwrap_or(0)}",
                                "{k}"
                            }
                        } else {
                            span { class: "tile",
                                Icon { name: h.icon, size: 17 }
                            }
                        }
                        span { class: "t1", "{h.t}" }
                        span { class: "k", "{h.k}" }
                    }
                }
            }
        }
    }
}

/// The player: the song big, the scrubber, the transport, what is next.
#[component]
fn Player() -> Element {
    let app = use_app();
    let mut np = app.np;
    let Some(n) = np() else { return rsx! {} };
    let Some(x) = n.song().copied() else { return rsx! {} };
    let len = x.len.unwrap_or(240) as f64;
    let pct = n.pos * 100.0;
    let next: Vec<Song> = n.list.iter().skip(n.i + 1).take(4).copied().collect();
    rsx! {
        BSheet { label: "Now playing", class: "player",
            div { class: "pfrom",
                "Playing from "
                b { "{n.from}" }
            }
            div { class: "partwork",
                SongArt { song: x }
            }
            div { class: "ptitle", "{x.title}" }
            div { class: "partist",
                "{x.artist.unwrap_or(\"\")}"
                if let Some(k) = x.key {
                    " · "
                    KeyC { k }
                }
                if let Some(t) = x.tempo {
                    " · {t} bpm"
                }
            }
            div { class: "pscrub",
                i { style: "width:{pct}%" }
                b { style: "left:{pct}%" }
            }
            div { class: "ptimes",
                span { "{dur(len * n.pos)}" }
                span { "-{dur(len * (1.0 - n.pos))}" }
            }
            div { class: "ptrans",
                button {
                    aria_label: "Previous",
                    onclick: move |_| {
                        np.with_mut(|n| {
                            if let Some(n) = n {
                                if n.pos > 0.05 || n.i == 0 {
                                    n.pos = 0.0;
                                } else {
                                    n.i -= 1;
                                }
                            }
                        })
                    },
                    Icon { name: I::SkipBack, size: 30, w: 2.0 }
                }
                button {
                    class: "pbig",
                    aria_label: if n.playing { "Pause" } else { "Play" },
                    onclick: move |_| np.with_mut(|n| if let Some(n) = n { n.playing = !n.playing }),
                    Icon { name: if n.playing { I::Pause } else { I::Play }, size: 34, w: 2.0 }
                }
                button {
                    aria_label: "Next",
                    onclick: move |_| {
                        np.with_mut(|n| {
                            if let Some(n) = n && n.i + 1 < n.list.len() {
                                n.i += 1;
                                n.pos = 0.0;
                            }
                        })
                    },
                    Icon { name: I::Skip, size: 30, w: 2.0 }
                }
            }
            if !next.is_empty() {
                div { class: "dayhead", "Up next" }
                for y in next {
                    div { key: "{y.title}", class: "upn",
                        SongArt { song: y }
                        span { class: "mid",
                            span { class: "t1", "{y.title}" }
                            span { class: "s1", "{y.artist.unwrap_or(\"\")}" }
                        }
                        if let Some(k) = y.key {
                            KeyC { k }
                        }
                    }
                }
            }
        }
    }
}

/// A song's key in a set: the twelve round the circle, how far from the original.
#[component]
fn KeySheet(set: String, i: usize) -> Element {
    let app = use_app();
    let Some(o) = data::occ(&set) else { return rsx! {} };
    let x = o.songs[i];
    let orig = x.key.unwrap_or("C");
    let cur = app.key_in(&set, i, &x).unwrap_or(orig);
    let pick = move |k: &'static str| {
        let mut ov = app.ov;
        let set = set.clone();
        move |_| {
            ov.with_mut(|m| m.entry((set.clone(), i)).or_insert_with(Override::default).key = Some(k));
            app.close();
        }
    };
    rsx! {
        BSheet { label: "Key",
            div { class: "bhead",
                b { "{x.title}" }
                span { class: "bsub", "Key · original {orig}" }
            }
            div { class: "keypick",
                for k in FIFTHS {
                    {
                        let d = data::steps(orig, k);
                        let mut cls = vec!["kp"];
                        if k == cur {
                            cls.push("on");
                        }
                        if k == orig {
                            cls.push("orig");
                        }
                        let sign = if d > 0 { "+" } else { "−" };
                        rsx! {
                            button {
                                key: "{k}",
                                class: "{cls.join(\" \")}",
                                style: "--h:{data::hue(k).unwrap_or(0)}",
                                onclick: pick(k),
                                b { "{k}" }
                                small {
                                    if k == orig {
                                        "Original"
                                    } else {
                                        "{sign}{d.abs()}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if cur != orig {
                button {
                    class: "secondary wide",
                    style: "margin-top:14px",
                    onclick: pick(orig),
                    "Back to {orig}, the original"
                }
            }
        }
    }
}

/// A song's arrangement in a set, and what each does to its length.
#[component]
fn ArrSheet(set: String, i: usize) -> Element {
    let app = use_app();
    let Some(o) = data::occ(&set) else { return rsx! {} };
    let x = o.songs[i];
    let cur = app.arr_in(o, i);
    let base = x.len.unwrap_or(270) as i32;
    rsx! {
        BSheet { label: "Arrangement",
            div { class: "bhead",
                b { "{x.title}" }
                span { class: "bsub", "Arrangement" }
            }
            for (a , dt) in data::ARRS {
                {
                    let set = set.clone();
                    let note = if dt == 0 {
                        format!("{} · as written", dur((base + dt) as f64))
                    } else {
                        format!(
                            "{} · {}{}",
                            dur((base + dt) as f64),
                            if dt > 0 { "+" } else { "−" },
                            dur(dt.abs() as f64),
                        )
                    };
                    rsx! {
                        button {
                            key: "{a}",
                            class: "mi big",
                            onclick: move |_| {
                                let mut ov = app.ov;
                                ov.with_mut(|m| m.entry((set.clone(), i)).or_insert_with(Override::default).arr = Some(a));
                                app.close();
                            },
                            span { class: "ic",
                                Icon { name: I::Branch, size: 18 }
                            }
                            span { class: "mid",
                                span { class: "t1", "{a}" }
                                span { class: "s1", "{note}" }
                            }
                            if a == cur {
                                span { class: "ck",
                                    Icon { name: I::Check, size: 17, w: 2.2 }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// More for a set: its arrangement, mixer and lighting.
#[component]
fn MoreSheet() -> Element {
    let app = use_app();
    rsx! {
        BSheet { label: "More",
            div { class: "bhead",
                b { "More for this set" }
            }
            for (l , i) in [
                ("Arrangement", I::Branch),
                ("Mixer", I::Sliders),
                ("Lighting visualizer", I::Bulb),
                ("Lighting cue list", I::Cues),
                ("Notes", I::Message),
            ]
            {
                button {
                    key: "{l}",
                    class: "mi big",
                    onclick: move |_| {
                        app.close();
                        app.say(l);
                    },
                    Icon { name: i, size: 20 }
                    "{l}"
                }
            }
        }
    }
}

/// Book a rehearsal: for which set, what day, what time, where.
#[component]
fn BookSheet() -> Element {
    let app = use_app();
    let mut booking = app.booking;
    let mut sets: Vec<&'static data::Occ> = data::OCC
        .iter()
        .filter(|o| (0..14).contains(&o.date.from_today()))
        .collect();
    sets.sort_by_key(|o| (o.mine().is_none(), o.date));
    sets.truncate(6);
    sets.sort_by_key(|o| o.date);
    // Nothing picked yet: your next set after today, the day before it.
    let fallback = sets
        .iter()
        .find(|o| o.mine().is_some() && o.date.from_today() >= 1)
        .or_else(|| sets.iter().find(|o| o.date.from_today() >= 1))
        .or(sets.first())
        .map(|o| Booking {
            set: o.id.clone(),
            day: (o.date.from_today() - 1).max(0) as usize,
            time: "7:00 PM",
            room: "Room 210",
        });
    let Some(b) = booking().or(fallback) else { return rsx! {} };
    let Some(o) = data::occ(&b.set) else { return rsx! {} };
    let last = o.date.from_today().max(0) as usize;
    let day = b.day.min(last);
    rsx! {
        BSheet { label: "Book a rehearsal",
            div { class: "bhead",
                b { "Book a rehearsal" }
            }
            div { class: "fsec", "For" }
            div { class: "seg",
                for x in sets {
                    button {
                        key: "{x.id}",
                        class: "fopt",
                        aria_pressed: "{b.set == x.id}",
                        onclick: {
                            let b = b.clone();
                            move |_| {
                                booking.set(Some(Booking {
                                    set: x.id.clone(),
                                    day: (x.date.from_today() - 1).max(0) as usize,
                                    ..b.clone()
                                }))
                            }
                        },
                        "{x.series.name}"
                        small { "{x.date.short()}" }
                    }
                }
            }
            div { class: "fsec", "Day" }
            div { class: "seg days",
                for k in 0..=last {
                    {
                        let d = TODAY.plus(k as i32);
                        let top = if k == 0 { "Today".to_string() } else { DAY[d.weekday()][..3].to_string() };
                        rsx! {
                            button {
                                key: "{k}",
                                class: "fopt",
                                aria_pressed: "{day == k}",
                                onclick: {
                                    let b = b.clone();
                                    move |_| booking.set(Some(Booking { day: k, ..b.clone() }))
                                },
                                "{top}"
                                small { "{MON[d.month0()]} {d.mday()}" }
                            }
                        }
                    }
                }
            }
            div { class: "fsec", "Time" }
            div { class: "seg",
                for t in ["10:00 AM", "1:00 PM", "6:00 PM", "7:00 PM"] {
                    button {
                        key: "{t}",
                        class: "fopt",
                        aria_pressed: "{b.time == t}",
                        onclick: {
                            let b = b.clone();
                            move |_| booking.set(Some(Booking { time: t, ..b.clone() }))
                        },
                        "{t}"
                    }
                }
            }
            div { class: "fsec", "Where" }
            div { class: "seg",
                for t in ["Room 210", "Chapel", "Worship Center", "The Loft"] {
                    button {
                        key: "{t}",
                        class: "fopt",
                        aria_pressed: "{b.room == t}",
                        onclick: {
                            let b = b.clone();
                            move |_| booking.set(Some(Booking { room: t, ..b.clone() }))
                        },
                        "{t}"
                    }
                }
            }
            button {
                class: "primary wide",
                onclick: move |_| {
                    let mut reh = app.reh;
                    reh.push(data::Rehearsal {
                        set: b.set.clone(),
                        date: TODAY.plus(day as i32),
                        time: b.time.into(),
                        room: b.room.into(),
                    });
                    app.close();
                    app.say("Rehearsal booked");
                },
                "Book"
            }
        }
    }
}

/// The library's filters: key, energy, tempo, what it is ready for, rotation.
#[component]
fn FilterSheet() -> Element {
    let app = use_app();
    let mut filters = app.filters;
    let f = filters();
    let n = data::LIB.iter().filter(|x| f.matches(x)).count();
    let s = if n == 1 { "" } else { "s" };
    rsx! {
        BSheet { label: "Filter songs",
            div { class: "bhead",
                b { "Filter" }
                button { onclick: move |_| filters.with_mut(|f| f.clear("all")), "Reset" }
            }
            div { class: "fsec", "Key" }
            div { class: "keygrid",
                for k in FIFTHS {
                    button {
                        key: "{k}",
                        class: "fopt",
                        style: "--h:{data::hue(k).unwrap_or(0)}",
                        aria_pressed: "{f.keys.contains(k)}",
                        onclick: move |_| filters.with_mut(|f| if !f.keys.remove(k) { f.keys.insert(k); }),
                        "{k}"
                    }
                }
                button {
                    class: "fopt",
                    aria_pressed: "{f.keys.contains(\"?\")}",
                    onclick: move |_| filters.with_mut(|f| if !f.keys.remove("?") { f.keys.insert("?"); }),
                    "None"
                }
            }
            div { class: "fsec", "Energy" }
            div { class: "seg",
                for e in 1..=5u8 {
                    button {
                        key: "{e}",
                        class: "fopt",
                        aria_pressed: "{f.energy.contains(&e)}",
                        onclick: move |_| filters.with_mut(|f| if !f.energy.remove(&e) { f.energy.insert(e); }),
                        Energy { e }
                    }
                }
            }
            div { class: "fsec", "Tempo" }
            div { class: "seg",
                for (nm , lo , hi) in data::TEMPI {
                    {
                        let range = if hi > 900 {
                            format!("{lo}+")
                        } else if lo > 0 {
                            format!("{lo}–{hi}")
                        } else {
                            format!("under {hi}")
                        };
                        rsx! {
                            button {
                                key: "{nm}",
                                class: "fopt",
                                aria_pressed: "{f.tempo.contains(nm)}",
                                onclick: move |_| filters.with_mut(|f| if !f.tempo.remove(nm) { f.tempo.insert(nm); }),
                                "{nm}"
                                small { "{range}" }
                            }
                        }
                    }
                }
            }
            div { class: "fsec", "Has" }
            div { class: "seg",
                for (k , nm) in data::READY {
                    button {
                        key: "{k}",
                        class: "fopt",
                        aria_pressed: "{f.ready.contains(&k)}",
                        onclick: move |_| filters.with_mut(|f| if !f.ready.remove(&k) { f.ready.insert(k); }),
                        "{nm}"
                    }
                }
            }
            div { class: "fsec", "Rotation" }
            div { class: "seg",
                button {
                    class: "fopt",
                    aria_pressed: "{f.rested}",
                    onclick: move |_| filters.with_mut(|f| f.rested = !f.rested),
                    "Not played in 4 weeks"
                }
            }
            button { class: "primary wide", onclick: move |_| app.close(), "Show {n} song{s}" }
        }
    }
}

/// Pick the org: all of them, or one.
#[component]
fn OrgSheet() -> Element {
    let app = use_app();
    let mut org = app.org;
    let cur = org();
    let count = |id: Option<&str>| {
        data::RECENT_WORK
            .iter()
            .filter(|r| id.is_none_or(|o| data::owner_of(r) == o))
            .count()
    };
    rsx! {
        BSheet { label: "Organization",
            div { class: "bhead",
                b { "Organization" }
            }
            button {
                class: "mi big",
                onclick: move |_| {
                    org.set(None);
                    app.close();
                },
                span { class: "omark all",
                    Icon { name: I::Layers, size: 15 }
                }
                "All organizations"
                span { class: "nt", "{count(None)}" }
                if cur.is_none() {
                    span { class: "ck",
                        Icon { name: I::Check, size: 17, w: 2.2 }
                    }
                }
            }
            div { class: "md" }
            for (id , name , h) in data::ORGS {
                {
                    let n = count(Some(id));
                    rsx! {
                        button {
                            key: "{id}",
                            class: "mi big",
                            onclick: move |_| {
                                org.set(Some(id));
                                app.close();
                            },
                            span { class: "omark", style: "--h:{h}", "{name.chars().next().unwrap_or('?')}" }
                            "{name}"
                            span { class: "nt",
                                if n > 0 {
                                    "{n}"
                                }
                            }
                            if cur == Some(id) {
                                span { class: "ck",
                                    Icon { name: I::Check, size: 17, w: 2.2 }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Change a track's status, or a record's ("rec:cr").
#[component]
fn StatusSheet(target: String) -> Element {
    let app = use_app();
    let (name, cur) = if let Some(a) = target.strip_prefix("rec:").and_then(data::album) {
        (a.title, app.status_of(a))
    } else if let Some(t) = data::track(&target) {
        (t.title, app.stage_of(t))
    } else {
        return rsx! {};
    };
    rsx! {
        BSheet { label: "Status",
            div { class: "bhead",
                b { "{name}" }
            }
            for (k , (nm , h)) in data::STAGES.iter().enumerate() {
                button {
                    key: "{nm}",
                    class: "mi big",
                    onclick: {
                        let target = target.clone();
                        move |_| {
                            let mut st = app.stages;
                            st.with_mut(|m| m.insert(target.clone(), k));
                            app.close();
                        }
                    },
                    span {
                        class: "sdot",
                        style: "--h:{h.unwrap_or(0)}",
                        "data-none": "{h.is_none()}",
                    }
                    "{nm}"
                    if k == cur {
                        span { class: "ck",
                            Icon { name: I::Check, size: 17, w: 2.2 }
                        }
                    }
                }
            }
        }
    }
}
