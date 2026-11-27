//! What opens over the page: the ⋯ menus from its bar, and the sheets —
//! open anything, the player, a song's key and arrangement in a set, More,
//! booking a rehearsal, the library's filters, the org.

use dioxus::prelude::*;

use crate::Route;
use crate::icons::{I, Icon};
use crate::look::{self, FIFTHS, dur};
use crate::pieces::{Energy, KeyC, SongArt};
use crate::song::Song;
use crate::state::{AppState, Menu, SetRef, Sheet, use_app};

/// A row of a menu.
enum Row {
    Div,
    /// Label, icon, note, what it says when tapped, red.
    Item(String, I, String, String, bool),
}

fn item(label: impl Into<String>, icon: I) -> Row {
    let l = label.into();
    Row::Item(l.clone(), icon, String::new(), l, false)
}

fn menu_rows(app: AppState, m: Menu) -> Vec<Row> {
    let _ = app;
    match m {
        Menu::SetMenu => vec![
            item("New set", I::Plus),
            item("Every setlist", I::Calendar),
            item("Library", I::Library),
        ],
        Menu::Series => vec![
            item("Plan a set", I::Plus),
            item("Book a rehearsal", I::Mic),
            item("Change the days", I::Settings),
            Row::Item(
                "Songs a set".into(),
                I::Setlist,
                "usual count".into(),
                "Songs a set".into(),
                false,
            ),
            Row::Div,
            item("Pin to the sidebar", I::Pin),
            item("Share…", I::Share),
        ],
        Menu::Occ => vec![
            item("The set's details", I::Pencil),
            item("Book a rehearsal", I::Mic),
            item("The band", I::Users),
            Row::Div,
            item("Share with another org", I::Share),
            item("Print or export", I::Printer),
        ],
    }
}

/// The open ⋯ menu, under the bar on the right; a tap outside closes it.
#[component]
pub fn MenuView() -> Element {
    let app = use_app();
    let mut menu = app.menu;
    let route = use_route::<Route>();
    let here = (app.here)();
    let Some(m) = menu() else { return rsx! {} };
    let rows = menu_rows(app, m);
    let set_here = route.set_id().map(|id| match &route {
        Route::OrgSet { owner, .. } => SetRef::new(owner, id),
        _ => SetRef::new(&here, id),
    });
    rsx! {
        div { class: "menu-catch", onclick: move |_| menu.set(None) }
        div { class: "menu",
            for (k , row) in rows.into_iter().enumerate() {
                match row {
                    Row::Div => rsx! {
                        div { key: "{k}", class: "md" }
                    },
                    Row::Item(label, icon, note, says, red) => {
                        let route = route.clone();
                        let set_here = set_here.clone();
                        rsx! {
                            button {
                                key: "{k}",
                                class: "mi",
                                style: if red { "color:#f87171" },
                                onclick: move |_| {
                                    menu.set(None);
                                    match says.as_str() {
                                        "Book a rehearsal" => {
                                            let mut b = app.booking;
                                            b.set(None);
                                            app.open(Sheet::Book);
                                        }
                                        "New set" => app.open(Sheet::NewSet(None)),
                                        "Plan a set" => app.open(Sheet::NewSet(match &route {
                                            Route::Series { id } => Some(id.clone()),
                                            _ => None,
                                        })),
                                        "The set's details" | "The band" | "Share with another org" | "Print or export" if set_here.is_some() => {
                                            let at = set_here.clone().unwrap_or_default();
                                            app.open(match says.as_str() {
                                                "The set's details" => Sheet::SetInfo(at),
                                                "The band" => Sheet::Band(at),
                                                "Share with another org" => Sheet::ShareSet(at),
                                                _ => Sheet::Export(at),
                                            });
                                        }
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

/// A sheet's content, drawn inside the shell's sheet (a bottom sheet on a
/// phone, a panel on a desktop).
#[component]
pub fn SheetBody(sheet: Sheet) -> Element {
    match sheet {
        Sheet::Palette => rsx! {
            Palette {}
        },
        Sheet::Player => rsx! {
            Player {}
        },
        Sheet::Key(set, entry) => rsx! {
            crate::set_sheets::KeySheet { set, entry }
        },
        Sheet::Arr(set, entry) => rsx! {
            crate::set_sheets::ArrSheet { set, entry }
        },
        Sheet::Entry(set, entry) => rsx! {
            crate::set_sheets::EntrySheet { set, entry }
        },
        Sheet::AddSongs(set) => rsx! {
            crate::set_sheets::AddSongs { set }
        },
        Sheet::Band(set) => rsx! {
            crate::set_sheets::BandSheet { set }
        },
        Sheet::SetInfo(set) => rsx! {
            crate::set_sheets::SetInfoSheet { set }
        },
        Sheet::ShareSet(set) => rsx! {
            crate::set_sheets::ShareSetSheet { set }
        },
        Sheet::Export(set) => rsx! {
            crate::set_sheets::ExportSheet { set }
        },
        Sheet::NewSet(preset) => rsx! {
            crate::set_sheets::NewSetSheet { preset }
        },
        Sheet::NewSession => rsx! {
            crate::new_session::NewSessionSheet {}
        },
        Sheet::More => rsx! {
            MoreSheet {}
        },
        Sheet::Book => rsx! {
            crate::set_sheets::BookSheet {}
        },
        Sheet::Filters => rsx! {
            FilterSheet {}
        },
        Sheet::Orgs => rsx! {
            OrgSheet {}
        },
        Sheet::Person(id) => rsx! {
            crate::pages::orgs::PersonSheet { id }
        },
        Sheet::OrgSong(owner, id) => rsx! {
            crate::pages::orgs::OrgSongSheet { owner, id }
        },
    }
}

/// A bottom sheet's card: taps inside stay inside.
#[component]
fn BSheet(
    label: String,
    #[props(default)] class: Option<&'static str>,
    children: Element,
) -> Element {
    let cls = match class {
        Some(c) => format!("bsheet {c}"),
        None => "bsheet".into(),
    };
    rsx! {
        div { class: "{cls}", role: "dialog", aria_label: "{label}", {children} }
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

fn palette_items(
    faces: &[crate::setlist::Face],
    files: &[crate::files::Hit],
    records: &[crate::records::Record],
    songs: &[crate::songs::Entry],
) -> Vec<Hit> {
    let mut v = Vec::new();
    let mut sets: Vec<&crate::setlist::Face> = faces
        .iter()
        .filter(|f| {
            f.series.is_empty() || f.day().is_some_and(|d| (0..8).contains(&d.from_today()))
        })
        .collect();
    sets.sort_by_key(|f| (!f.upcoming(), f.date.clone()));
    for f in sets {
        v.push(Hit {
            t: match f.day() {
                Some(d) if !f.series.is_empty() => format!("{} · {}", f.title, d.short()),
                _ => f.title.clone(),
            },
            k: "Set",
            open: Some(Route::SetPage { id: f.id.clone() }),
            says: String::new(),
            key: None,
            icon: I::Setlist,
        });
    }
    for h in files {
        v.push(Hit {
            t: crate::setfile::name_of(&h.name),
            k: "Setlist",
            open: Some(Route::DriveSet {
                space: h.space.clone(),
                location: h.location.clone(),
                path: h.path.split('/').map(str::to_string).collect(),
            }),
            says: String::new(),
            key: None,
            icon: I::Setlist,
        });
    }
    for r in records {
        v.push(Hit {
            t: r.title.clone(),
            k: "Record",
            open: Some(Route::Album { id: r.id.clone() }),
            says: String::new(),
            key: None,
            icon: I::Disc,
        });
    }
    for s in songs {
        v.push(Hit {
            t: s.title.clone(),
            k: "Song",
            open: Some(Route::SongPage { id: s.id.clone() }),
            says: String::new(),
            key: Some(s.key.as_deref().and_then(look::norm_key).unwrap_or("?")),
            icon: I::Audio,
        });
    }
    for a in ["New session"] {
        v.push(Hit {
            t: a.into(),
            k: "Action",
            open: None,
            says: a.into(),
            key: None,
            icon: I::Command,
        });
    }
    v
}

/// Open anything: a search from the top over everything there is.
#[component]
fn Palette() -> Element {
    let app = use_app();
    let mut q = use_signal(String::new);
    let index = crate::sets::use_index();
    let lib = crate::songs::use_library();
    let here = (app.here)();
    let files: Vec<crate::files::Hit> = (app.drive_sets)()
        .unwrap_or_default()
        .into_iter()
        .filter(|h| h.space == here)
        .collect();
    let records = crate::pages::records::of_org(&(app.records)().unwrap_or_default(), Some(&here));
    let all = palette_items(&index.faces(), &files, &records, &lib.entries());
    let needle = q().trim().to_lowercase();
    let list: Vec<Hit> = if needle.is_empty() {
        all.iter().take(6).cloned().collect()
    } else {
        all.iter()
            .filter(|h| h.t.to_lowercase().contains(&needle))
            .take(12)
            .cloned()
            .collect()
    };
    rsx! {
        div { class: "panel", onclick: move |e| e.stop_propagation(),
            label { class: "field",
                Icon { name: I::Search, size: 18 }
                input {
                    placeholder: "Setlists, records, songs, actions",
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
                                    None if h.says == "New session" => app.open(Sheet::NewSession),
                                    None => app.say(h.says.clone()),
                                }
                            }
                        },
                        if let Some(k) = h.key {
                            span {
                                class: if k == "?" { "tile nokey" } else { "tile key" },
                                style: "--h:{look::hue(k).unwrap_or(0)}",
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
    let in_session = matches!(
        use_route::<crate::Route>(),
        crate::Route::SessionPage { .. }
    );
    let Some(n) = np() else { return rsx! {} };
    let Some(x) = n.song().copied() else {
        return rsx! {};
    };
    let (now, len) = n.secs();
    let pct = n.pos * 100.0;
    let line = crate::shell::now_line(&n);
    let sections = n.timeline.sections.clone();
    let here = n.timeline.at(now).map(|s| s.start);
    let mut seek = app.seek;
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
                for sec in sections.iter().filter(|s| s.start > 0.0) {
                    u { key: "{sec.start}", style: "left:{sec.start / len * 100.0}%" }
                }
                b { style: "left:{pct}%" }
                input {
                    r#type: "range",
                    class: "pseek",
                    aria_label: "Position",
                    min: "0",
                    max: "1000",
                    value: "{(n.pos * 1000.0).round()}",
                    onchange: move |e| {
                        if let Ok(v) = e.value().parse::<f64>() {
                            seek.set(Some(v / 1000.0 * len));
                        }
                    },
                }
            }
            div { class: "ptimes",
                span { "{dur(now)}" }
                span { "-{dur(len - now)}" }
            }
            div { class: "pline", "{line}" }
            div { class: "ptrans",
                button {
                    aria_label: "Previous",
                    onclick: move |_| {
                        np.with_mut(|n| {
                            if let Some(n) = n {
                                if n.pos > 0.05 || n.i == 0 {
                                    n.pos = 0.0;
                                    seek.set(Some(0.0));
                                } else {
                                    n.i -= 1;
                                    n.pos = 0.0;
                                    n.dur = 0.0;
                                    n.timeline = Default::default();
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
                            if let Some(n) = n {
                                n.next();
                            }
                        })
                    },
                    Icon { name: I::Skip, size: 30, w: 2.0 }
                }
            }
            // Into the session (a phone has no top bar's Session button).
            if !in_session {
                div { class: "psess",
                    for v in [crate::session::View::Arrangement, crate::session::View::Mixer, crate::session::View::Chart] {
                        button {
                            key: "{v.slug()}",
                            class: "secondary",
                            "data-open": v.slug(),
                            onclick: move |_| {
                                app.close();
                                navigator().push(crate::session::route(v));
                            },
                            "{v.name()}"
                        }
                    }
                }
            }
            if !sections.is_empty() {
                div { class: "dayhead", "Sections" }
                div { class: "psecs",
                    for sec in sections {
                        button {
                            key: "{sec.start}",
                            class: if here == Some(sec.start) { "psec on" } else { "psec" },
                            onclick: move |_| seek.set(Some(sec.start)),
                            span { class: "t1", "{sec.name}" }
                            span { class: "s1", "{dur(sec.start)}" }
                        }
                    }
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

/// More for a set: its arrangement, mixer and lighting.
#[component]
fn MoreSheet() -> Element {
    let app = use_app();
    rsx! {
        BSheet { label: "More",
            div { class: "bhead",
                b { "More for this set" }
            }
            for (l , i , view) in [
                ("Arrangement", I::Branch, crate::session::View::Arrangement),
                ("Mixer", I::Sliders, crate::session::View::Mixer),
            ]
            {
                button {
                    key: "{l}",
                    class: "mi big",
                    onclick: move |_| {
                        app.close();
                        // The arrangement and the mixer are the session's:
                        // into it, the set playing (from its top if nothing
                        // is yet).
                        if !app.np.peek().as_ref().is_some_and(|n| n.song().is_some()) {
                            let titles: Vec<String> = app
                                .live_list
                                .peek()
                                .iter()
                                .map(|s| s.title.clone())
                                .collect();
                            app.play_titles(&titles, "This set".into(), None);
                        }
                        navigator().push(crate::session::route(view));
                    },
                    Icon { name: i, size: 20 }
                    "{l}"
                }
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
    let lib = crate::songs::use_library();
    let n = lib.entries().iter().filter(|x| x.admitted(&f)).count();
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
                        style: "--h:{look::hue(k).unwrap_or(0)}",
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
                for (nm , lo , hi) in look::TEMPI {
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
                for (k , nm) in look::READY {
                    button {
                        key: "{k}",
                        class: "fopt",
                        aria_pressed: "{f.ready.contains(&k)}",
                        onclick: move |_| filters.with_mut(|f| if !f.ready.remove(&k) { f.ready.insert(k); }),
                        "{nm}"
                    }
                }
            }
            button { class: "primary wide", onclick: move |_| app.close(), "Show {n} song{s}" }
        }
    }
}

/// Pick the org: all of them (the All orgs page), or one.
#[component]
fn OrgSheet() -> Element {
    let app = use_app();
    let cur = (app.org)();
    let list = (app.orgs)();
    rsx! {
        BSheet { label: "Organization",
            div { class: "bhead",
                b { "Organization" }
            }
            button {
                class: "mi big",
                onclick: move |_| {
                    let mut org = app.org;
                    org.set(None);
                    app.close();
                    navigator().push(Route::AllOrgs {});
                },
                span { class: "omark all",
                    Icon { name: I::Layers, size: 15 }
                }
                "All organizations"
                span { class: "nt", "{list.len()}" }
                if cur.is_none() {
                    span { class: "ck",
                        Icon { name: I::Check, size: 17, w: 2.2 }
                    }
                }
            }
            div { class: "md" }
            for id in list.iter().cloned() {
                {
                    let name = crate::orgs::org_name(&id);
                    let h = crate::orgs::org_hue(&id);
                    let on = cur.as_deref() == Some(id.as_str());
                    rsx! {
                        button {
                            key: "{id}",
                            class: "mi big",
                            onclick: move |_| {
                                app.close();
                                app.switch_org(&id);
                            },
                            span { class: "omark", style: "--h:{h}", "{name.chars().next().unwrap_or('?')}" }
                            "{name}"
                            if on {
                                span { class: "ck",
                                    Icon { name: I::Check, size: 17, w: 2.2 }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "md" }
            crate::pages::orgs::NewOrg { done: move |()| app.close() }
            crate::pages::orgs::JoinOrg { done: move |()| app.close() }
        }
    }
}
