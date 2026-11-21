//! Home: what needs you now, the live side and the studio side.

use dioxus::prelude::*;

use super::{BookButton, OccRow, OrgPill, RehRow, progress_of};
use crate::Route;
use crate::data::{self, DAY, MONTH, Rehearsal, TODAY};
use crate::icons::{I, Icon};
use crate::pieces::{Art, Bar, Page};
use crate::state::use_app;

/// A row of the week: a set or a rehearsal.
#[derive(Clone, PartialEq)]
enum Agenda {
    Set(String),
    Reh(Rehearsal),
}

#[component]
pub fn Home() -> Element {
    let app = use_app();
    let mine = data::my_sets();
    let next = mine.first().copied();
    let mut seen = vec![next.map(|n| n.series.id)];
    let mut week: Vec<(data::Day, Agenda)> = Vec::new();
    for o in mine.iter().skip(1) {
        if o.date.from_today() < 8 && !seen.contains(&Some(o.series.id)) {
            seen.push(Some(o.series.id));
            week.push((o.date, Agenda::Set(o.id.clone())));
        }
    }
    for r in app.reh.read().iter() {
        let n = r.date.from_today();
        if (0..8).contains(&n) {
            week.push((r.date, Agenda::Reh(r.clone())));
        }
    }
    week.sort_by_key(|w| w.0);
    week.truncate(3);

    let review: Vec<&'static data::Track> = data::STUDIO
        .iter()
        .filter(|x| {
            let when = app.latest(x.title).map(|v| v.when).unwrap_or_default();
            app.open_on(x.title) > 0 || when.ends_with('h') || when == "Yesterday"
        })
        .take(3)
        .collect();

    let date = format!(
        "{}, {} {}",
        DAY[TODAY.weekday()],
        MONTH[TODAY.month0()],
        TODAY.mday()
    );

    rsx! {
        Page {
            Bar { title: "Home", tools: rsx! {
                OrgPill {}
            } }
            div { class: "body",
                div { class: "hdate", "{date}" }
                if let Some(o) = next {
                    NextUp { id: o.id.clone() }
                }
                if !week.is_empty() {
                    div { class: "dayhead dh2",
                        span { "This week" }
                        BookButton {}
                    }
                    div { class: "agenda",
                        for (_ , a) in week {
                            match a {
                                Agenda::Set(id) => rsx! {
                                    OccRow { key: "{id}", id: id.clone(), state: progress_of(data::occ(&id).expect("a set")) }
                                },
                                Agenda::Reh(r) => rsx! {
                                    RehRow { key: "reh{r.set}{r.date.0}", r }
                                },
                            }
                        }
                    }
                }
                if !review.is_empty() {
                    div { class: "dayhead dh2",
                        span { "To review" }
                        button {
                            class: "seeall",
                            onclick: move |_| {
                                navigator().push(Route::Records {});
                            },
                            "Records "
                            Icon { name: I::ChevRight, size: 14 }
                        }
                    }
                    div { class: "agenda",
                        for x in review {
                            ReviewRow { key: "{x.idx}", idx: x.idx }
                        }
                    }
                }
                div { class: "dayhead", "Jump back in" }
                div { class: "shelf",
                    for (open , title , kind) in [
                        ("jt", "JT's Senior Recital", "Setlist"),
                        ("cr", "Crescendum", "Album"),
                        ("t:Fire", "Fire", "Single"),
                        ("mb", "Merry & Bright", "Setlist"),
                        ("png", "PNG Worship", "Album"),
                    ]
                    {
                        ShelfItem { key: "{open}", open, title, kind }
                    }
                }
            }
        }
    }
}

/// Tonight's set (or the next one you play), its room, call and rig.
#[component]
fn NextUp(id: String) -> Element {
    let Some(o) = data::occ(&id) else {
        return rsx! {};
    };
    let pl = o.series.place();
    let kicker = if o.date.from_today() == 0 {
        "Tonight".to_string()
    } else {
        DAY[o.date.weekday()].to_string()
    };
    let kicker = match pl {
        Some(p) => format!("{kicker} · {}", p.starts),
        None => kicker,
    };
    let folder = o.series.folder().map_or("", |f| f.name);
    let loaded = data::loaded_rig();
    rsx! {
        button {
            class: "nextup hn",
            style: "--h:{o.series.hue}",
            onclick: move |_| {
                navigator().push(Route::set(&id));
            },
            span { class: "nutop",
                if let Some(l) = data::logo(o.series.id) {
                    img { class: "nulogo", src: l, alt: "" }
                } else {
                    crate::pieces::Cover { title: o.series.name, size: 64, hue: o.series.hue }
                }
                span { class: "mid",
                    span { class: "nuk", "{kicker}" }
                    span { class: "nut", "{o.series.name}" }
                    span { class: "nus", "{folder} · {o.songs.len()} songs" }
                }
            }
            span { class: "nuwhere",
                span { class: "nw",
                    Icon { name: I::Mappin, size: 13, w: 2.0 }
                    span {
                        b { "{pl.map_or(\"\", |p| p.room)}" }
                    }
                }
                span { class: "nw call",
                    Icon { name: I::Clock, size: 13, w: 2.0 }
                    span {
                        small { "Call" }
                        b { "{pl.map_or(\"\", |p| p.call)}" }
                    }
                }
                if let Some(r) = loaded {
                    span { class: "nw rigc", style: "--h:{r.hue}",
                        Icon { name: r.icon, size: 13, w: 2.0 }
                        span {
                            b { "{r.name}" }
                            small { "loaded" }
                        }
                    }
                }
                span { class: "nuready", {progress_of(o)} }
            }
        }
    }
}

/// A track waiting on you: its latest version and what is open on it.
#[component]
fn ReviewRow(idx: usize) -> Element {
    let app = use_app();
    let x = &data::STUDIO[idx];
    let v = app.latest(x.title);
    let n = app.open_on(x.title);
    rsx! {
        button {
            class: "ag",
            onclick: move |_| {
                navigator().push(Route::TrackPage { idx });
            },
            TrackCover { idx, size: 40 }
            span { class: "mid",
                span { class: "t1", "{x.title}" }
                if let Some(v) = v {
                    span { class: "s1", "{v.what} v{v.v} · {v.who} · {v.when}" }
                }
            }
            if n > 0 {
                span { class: "bubble",
                    Icon { name: I::Message, size: 13, w: 2.0 }
                    "{n}"
                }
            } else {
                span { class: "newv", "New" }
            }
        }
    }
}

/// A track's art: its own, else its record's, else a generated one.
#[component]
pub fn TrackCover(idx: usize, size: u32) -> Element {
    let x = &data::STUDIO[idx];
    rsx! {
        if let Some(a) = x.art() {
            img {
                class: "tcov",
                src: a,
                alt: "",
                style: "width:{size}px;height:{size}px",
            }
        } else {
            crate::pieces::Cover { title: x.cover_title(), size, hue: x.cover_hue() }
        }
    }
}

#[component]
fn ShelfItem(open: &'static str, title: &'static str, kind: &'static str) -> Element {
    let hue = data::album(open)
        .map(|a| a.hue)
        .or_else(|| open.strip_prefix("t:").and_then(data::track).map(|t| t.hue))
        .unwrap_or(150);
    let img = data::art(open);
    let cover_title = data::album(open).map_or(title, |a| a.title);
    rsx! {
        button {
            class: "shelfi",
            onclick: move |_| {
                navigator().push(Route::open(open));
            },
            span { class: "sc",
                Art { img, title: cover_title, hue }
            }
            span { class: "t1", "{title}" }
            span { class: "s1", "{kind}" }
        }
    }
}
