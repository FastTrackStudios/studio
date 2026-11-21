//! Setlists: the weekly events as their logos, each ringed by how ready
//! its next set is; the shows as their covers. A series' page, and every
//! setlist there is.

use dioxus::prelude::*;

use super::{book_for, progress_of};
use crate::Route;
use crate::data::{self, Day, MONTH};
use crate::icons::{I, Icon};
use crate::pieces::{Art, Bar, Cal, Cover, DayTile, Hair, Hero, Joined, Page, Ring, Tool};
use crate::state::{Menu, use_app};

#[component]
pub fn Setlists() -> Element {
    let app = use_app();
    let mut folder = app.folder;
    let f = folder();
    let fs: Vec<&data::Folder> = data::FOLDERS
        .iter()
        .filter(|x| f.is_none_or(|t| t == x.tag))
        .collect();
    let shows: Vec<(&'static str, &'static data::Folder)> = fs
        .iter()
        .flat_map(|x| x.shows.iter().map(move |id| (*id, *x)))
        .collect();
    let upcoming: Vec<_> = shows
        .iter()
        .copied()
        .filter(|(id, _)| data::show(id).is_some_and(|s| s.upcoming))
        .collect();
    let past: Vec<_> = shows
        .iter()
        .copied()
        .filter(|(id, _)| data::show(id).is_some_and(|s| !s.upcoming))
        .collect();
    let recurring: Vec<_> = fs.iter().filter(|x| !x.series.is_empty()).collect();

    rsx! {
        Page {
            Bar {
                title: "Setlists",
                tools: rsx! {
                    Joined {
                        Tool {
                            label: "Library",
                            icon: I::Library,
                            onclick: move |_| {
                                navigator().push(Route::LibraryHome {});
                            },
                        }
                        Hair {}
                        Tool {
                            label: "Folders and every setlist",
                            icon: I::More,
                            badge: f.map(|_| "1".to_string()),
                            onclick: move |_| {
                                let mut m = app.menu;
                                m.set(Some(Menu::SetMenu));
                            },
                        }
                    }
                    Tool {
                        label: "New setlist",
                        icon: I::Plus,
                        onclick: move |_| {
                            let mut m = app.menu;
                            m.set(Some(Menu::NewSet));
                        },
                    }
                },
            }
            div { class: "body",
                if let Some(tag) = f {
                    div { class: "onrow",
                        button { class: "onchip", onclick: move |_| folder.set(None),
                            "{data::FOLDERS.iter().find(|x| x.tag == tag).map_or(\"\", |x| x.name)}"
                            Icon { name: I::X, size: 13, w: 2.4 }
                        }
                    }
                }
                for (k , fo) in recurring.into_iter().enumerate() {
                    Fragment { key: "{fo.tag}",
                        div { class: "dayhead dh2",
                            span { "{fo.name} · Recurring" }
                            if k == 0 {
                                button {
                                    class: "seeall",
                                    onclick: move |_| {
                                        navigator().push(Route::Every {});
                                    },
                                    "Every setlist "
                                    Icon { name: I::ChevRight, size: 14 }
                                }
                            }
                        }
                        div { class: "evgrid",
                            for id in sorted_series(fo) {
                                EventTile { key: "{id}", id }
                            }
                        }
                    }
                }
                for (head , list , cls) in [("Upcoming shows", upcoming, "showlist"), ("Past shows", past, "showlist past")] {
                    if !list.is_empty() {
                        Fragment { key: "{head}",
                            div { class: "dayhead", "{head}" }
                            div { class: "{cls}",
                                for (id , fo) in list {
                                    ShowRow { key: "{id}", id, folder: fo.name }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A folder's weekly events, the soonest next set first.
fn sorted_series(f: &data::Folder) -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = f.series.to_vec();
    ids.sort_by_key(|id| data::series(id).and_then(|s| s.next()).map(|o| o.date));
    ids
}

/// A weekly event: its logo ringed by how ready its next set is.
#[component]
fn EventTile(id: &'static str) -> Element {
    let Some(sr) = data::series(id) else {
        return rsx! {};
    };
    let nx = sr.next();
    let (n, of) = (nx.map_or(0, |o| o.songs.len()), sr.size);
    let when = nx.map(|o| {
        let d = o.date.from_today();
        if d == 0 {
            "Today".to_string()
        } else if d < 7 {
            o.date.dow3().to_string()
        } else {
            o.date.short()
        }
    });
    let cls = if n >= of {
        "ok"
    } else if n > 0 {
        ""
    } else {
        "none"
    };
    rsx! {
        button {
            class: "ev",
            onclick: move |_| {
                navigator().push(Route::Series { id: id.into() });
            },
            span { class: "evlogo",
                Ring { n, of, size: 84 }
                if let Some(l) = data::logo(id) {
                    img { src: l, alt: "" }
                } else {
                    Cover { title: sr.name, size: 64, hue: sr.hue }
                }
            }
            span { class: "t1", "{sr.name}" }
            span { class: "s1",
                if let Some(w) = when {
                    "{w} · "
                    b { class: "{cls}", "{n}/{of}" }
                }
            }
        }
    }
}

/// A show, its cover and its date.
#[component]
fn ShowRow(id: &'static str, folder: &'static str) -> Element {
    let Some(st) = data::show(id) else {
        return rsx! {};
    };
    let total = st.total();
    let n = st.songs().len();
    rsx! {
        button {
            class: "showc",
            onclick: move |_| {
                navigator().push(Route::set(id));
            },
            span { class: "showart",
                Art { img: data::art(id), title: st.title, hue: st.hue }
            }
            span { class: "mid",
                span { class: "t1", "{st.title}" }
                span { class: "s1",
                    "{folder} · {n} songs"
                    if total > 0 {
                        " · {data::dur(total as f64)}"
                    }
                }
            }
            Cal { top: st.cal.0, big: st.cal.1 }
        }
    }
}

/// A weekly event: when and where it meets, its sets to come and before.
#[component]
pub fn Series(id: String) -> Element {
    let app = use_app();
    let Some(sr) = data::series(&id) else {
        return rsx! {
            Page { Bar { back: Route::Setlists {}, title: "Not found" } }
        };
    };
    let all = sr.occs();
    let up: Vec<_> = all
        .iter()
        .copied()
        .filter(|o| o.date.from_today() >= 0)
        .collect();
    let mut past: Vec<_> = all
        .iter()
        .copied()
        .filter(|o| o.date.from_today() < 0)
        .collect();
    past.reverse();
    let pl = sr.place();
    let folder = sr.folder().map_or("", |f| f.name);
    let nx_say = up.first().map_or("Plan a set".to_string(), |o| {
        format!("Planning {}", o.date.short())
    });
    let past_n = past.len();
    rsx! {
        Page { tint: sr.hue,
            Bar {
                back: Route::Setlists {},
                tools: rsx! {
                    Joined {
                        Tool {
                            label: "Plan a set",
                            icon: I::Plus,
                            onclick: move |_| app.say(nx_say.clone()),
                        }
                        Hair {}
                        Tool {
                            label: "More",
                            icon: I::More,
                            onclick: move |_| {
                                let mut m = app.menu;
                                m.set(Some(Menu::Series));
                            },
                        }
                    }
                },
            }
            div { class: "body",
                Hero {
                    img: data::logo(sr.id),
                    hue: sr.hue,
                    title: sr.name,
                    facts: rsx! {
                        span { class: "when2",
                            span { class: "loc",
                                Icon { name: I::Mappin, size: 14, w: 2.0 }
                                span {
                                    b { "{pl.map_or(\"\", |p| p.room)}" }
                                    small {
                                        "{sr.every()}"
                                        if let Some(p) = pl {
                                            " · "
                                            span { class: "call", "Call {p.call}" }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    sub: rsx! {
                        "{folder}"
                        span { class: "hcount", "{sr.size} songs a set · {past_n} sets so far" }
                    },
                }
                div { class: "dayhead", "Coming up" }
                div { class: "agenda",
                    for o in up {
                        SeriesRow { key: "{o.id}", id: o.id.clone() }
                    }
                }
                div { class: "dayhead", "Before" }
                div { class: "agenda",
                    for o in past {
                        SeriesRow { key: "{o.id}", id: o.id.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn SeriesRow(id: String) -> Element {
    let Some(o) = data::occ(&id) else {
        return rsx! {};
    };
    let names: Vec<_> = o.songs.iter().take(3).map(|x| x.title).collect();
    rsx! {
        button {
            class: "ag",
            onclick: move |_| {
                navigator().push(Route::set(&id));
            },
            DayTile { day: o.date }
            span { class: "mid",
                span { class: "t1", "{o.date.short()}" }
                span { class: "s1",
                    if names.is_empty() {
                        "No songs yet"
                    } else {
                        "{names.join(\", \")}"
                    }
                }
            }
            if o.date.from_today() >= 0 {
                {progress_of(o)}
            } else {
                span { class: "chev",
                    Icon { name: I::ChevRight, size: 16 }
                }
            }
        }
    }
}

/// A row of every setlist.
struct EveryRow {
    date: Day,
    title: &'static str,
    sub: String,
    img: Option<Asset>,
    open: String,
    occ: Option<&'static data::Occ>,
    future: bool,
}

/// Every setlist there is, by month, newest first, with a search.
#[component]
pub fn Every() -> Element {
    let app = use_app();
    let mut q = app.every_q;
    let mut rows: Vec<EveryRow> = data::OCC
        .iter()
        .filter(|o| !o.songs.is_empty() || (0..7).contains(&o.date.from_today()))
        .map(|o| {
            let first: Vec<_> = o.songs.iter().take(2).map(|x| x.title).collect();
            EveryRow {
                date: o.date,
                title: o.series.name,
                sub: if o.songs.is_empty() {
                    "Not planned yet".into()
                } else {
                    format!("{} songs · {}", o.songs.len(), first.join(", "))
                },
                img: data::logo(o.series.id),
                open: o.id.clone(),
                occ: Some(o),
                future: o.date.from_today() >= 0,
            }
        })
        .collect();
    rows.push(EveryRow {
        date: Day::ymd(2026, 12, 12),
        title: "Merry & Bright",
        sub: "CBU · 9 songs".into(),
        img: None,
        open: "mb".into(),
        occ: None,
        future: true,
    });
    rows.push(EveryRow {
        date: Day::ymd(2026, 5, 15),
        title: "JT's Senior Recital",
        sub: "Recitals · 16 songs".into(),
        img: data::art("jt"),
        open: "jt".into(),
        occ: None,
        future: false,
    });
    let n_all = rows.len();
    let needle = q().to_lowercase();
    rows.retain(|r| needle.is_empty() || r.title.to_lowercase().contains(&needle));
    rows.sort_by(|a, b| b.date.cmp(&a.date));
    let mut months: Vec<(i32, usize)> = Vec::new();
    for r in &rows {
        let m = (r.date.year(), r.date.month0());
        if !months.contains(&m) {
            months.push(m);
        }
    }
    rsx! {
        Page {
            Bar { back: Route::Setlists {}, title: "Every setlist" }
            div { class: "body",
                label { class: "search lib",
                    Icon { name: I::Search, size: 16 }
                    input {
                        r#type: "search",
                        placeholder: "Search {n_all} setlists",
                        value: "{q}",
                        aria_label: "Search setlists",
                        oninput: move |e| q.set(e.value()),
                    }
                }
                for (y , mo) in months {
                    div { key: "{y}-{mo}h", class: "dayhead", "{MONTH[mo]} {y}" }
                    div { class: "agenda",
                        for r in rows.iter().filter(|r| r.date.year() == y && r.date.month0() == mo) {
                            button {
                                key: "{r.open}",
                                class: "ag",
                                onclick: {
                                    let open = r.open.clone();
                                    move |_| {
                                        navigator().push(Route::set(&open));
                                    }
                                },
                                DayTile { day: r.date }
                                if let Some(i) = r.img {
                                    img { class: "aglogo", src: i, alt: "" }
                                } else {
                                    span { class: "aglogo",
                                        Cover { title: r.title, size: 36, hue: 150 }
                                    }
                                }
                                span { class: "mid",
                                    span { class: "t1", "{r.title}" }
                                    span { class: "s1", "{r.sub}" }
                                }
                                if let (Some(o), true) = (r.occ, r.future) {
                                    {progress_of(o)}
                                } else if r.future {
                                    span { class: "st ok",
                                        Icon { name: I::Check, size: 13, w: 2.6 }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Book a rehearsal for the set on show, or the series' next.
pub fn book_here(app: crate::state::AppState, route: &Route) {
    let o = match route {
        Route::SetPage { id } => data::occ(id),
        Route::Series { id } => data::series(id).and_then(|s| s.next()),
        _ => None,
    }
    .or_else(|| data::SERIES[0].next());
    if let Some(o) = o {
        book_for(app, o);
    }
}
