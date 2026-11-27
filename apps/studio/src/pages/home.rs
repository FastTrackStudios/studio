//! Home: what needs you now, the live side and the studio side.

use dioxus::prelude::*;

use super::{BookButton, FaceRow, OrgPill, RehRow, progress_of};
use crate::Route;
use crate::data::{self, DAY, MONTH, TODAY};
use crate::icons::{I, Icon};
use crate::pieces::{Art, Bar, Page};
use crate::setlist::{Face, Rehearsal};
use crate::state::use_app;

/// A row of the week: a set or a rehearsal.
#[derive(Clone, PartialEq)]
enum Agenda {
    Set(Face),
    Reh(Face, Rehearsal),
}

#[component]
pub fn Home() -> Element {
    let app = use_app();
    let index = crate::sets::use_index();
    let me = crate::team::use_me();
    let faces = index.faces();
    let mine = crate::sheets::my_sets(&faces, me.as_deref());
    let next = mine.first().cloned();
    let mut seen = vec![next.as_ref().map(|n| n.series.clone())];
    let mut week: Vec<(data::Day, Agenda)> = Vec::new();
    for f in mine.iter().skip(1) {
        let Some(d) = f.day() else { continue };
        let key = Some(f.series.clone()).filter(|s| !s.is_empty());
        if d.from_today() < 8 && (key.is_none() || !seen.contains(&key)) {
            seen.push(key);
            week.push((d, Agenda::Set(f.clone())));
        }
    }
    for f in &faces {
        for r in &f.rehearsals {
            if let Some(d) = crate::setlist::parse_day(&r.date)
                && (0..8).contains(&d.from_today())
            {
                week.push((d, Agenda::Reh(f.clone(), r.clone())));
            }
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
                YourOrgs {}
                if let Some(f) = next {
                    NextUp { face: f }
                }
                if !week.is_empty() {
                    div { class: "dayhead dh2",
                        span { "This week" }
                        BookButton {}
                    }
                    div { class: "agenda",
                        for (_ , a) in week {
                            match a {
                                Agenda::Set(f) => rsx! {
                                    FaceRow { key: "{f.id}", face: f.clone(), state: progress_of(&f) }
                                },
                                Agenda::Reh(f, r) => rsx! {
                                    RehRow { key: "reh{f.id}{r.date}{r.time}", face: f, r }
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

/// The first run's way in (studio#699): each org this person is in, with
/// its setlists one tap from open; in none, a new one (or the invitation
/// someone sent). Only with Drive, whose orgs are the account's.
#[cfg(feature = "drive")]
#[component]
fn YourOrgs() -> Element {
    use crate::first_run::{self, Step};
    let app = use_app();
    let index = crate::sets::use_index();
    let orgs = (app.orgs)();
    let here = (app.here)();
    let sets: Vec<(String, String)> = index.faces().into_iter().map(|f| (f.id, f.title)).collect();
    let files = (app.drive_sets)().unwrap_or_default();
    let listed = first_run::offers(&orgs, &here, &sets, &files);
    let count = listed.iter().map(|o| o.setlists.len()).sum();
    match first_run::step(false, &orgs, count) {
        Step::SignIn => rsx! {},
        Step::NoOrganization => rsx! {
            div { class: "dayhead", "Welcome to Studio" }
            p { class: "lede",
                "You are not in an organization yet. Make one for your band, church or school, or open the invitation someone sent you."
            }
            crate::pages::orgs::NewOrg {}
        },
        Step::NoSetlist | Step::Ready => rsx! {
            div { class: "dayhead", "Your organizations" }
            for o in listed {
                OrgSetlists { key: "{o.org}", org: o }
            }
        },
    }
}

#[cfg(not(feature = "drive"))]
#[component]
fn YourOrgs() -> Element {
    rsx! {}
}

/// One org and its setlists, each opening where it is kept.
#[cfg(feature = "drive")]
#[component]
fn OrgSetlists(org: crate::first_run::OrgSetlists) -> Element {
    let name = crate::orgs::org_name(&org.org);
    let none = org.setlists.is_empty();
    let count = org.setlists.len();
    rsx! {
        div { class: "agenda",
            div { class: "ag",
                crate::pages::orgs::OrgMark { org: org.org.clone(), size: 28 }
                span { class: "mid",
                    span { class: "t1", "{name}" }
                    span { class: "s1",
                        if none {
                            "No setlists yet"
                        } else {
                            "{count} setlists"
                        }
                    }
                }
            }
            for s in org.setlists {
                button {
                    key: "{s.open}",
                    class: "ag",
                    onclick: {
                        let to = s.open.clone();
                        move |_| {
                            navigator().push(to.clone());
                        }
                    },
                    span { class: "cal tpl",
                        Icon { name: I::Setlist, size: 18 }
                    }
                    span { class: "mid",
                        span { class: "t1", "{s.title}" }
                    }
                    span { class: "chev",
                        Icon { name: I::ChevRight, size: 16 }
                    }
                }
            }
            if none {
                button {
                    class: "ag",
                    onclick: move |_| {
                        navigator().push(Route::Setlists {});
                    },
                    span { class: "mid",
                        span { class: "t1", "Make a setlist" }
                    }
                    span { class: "chev",
                        Icon { name: I::ChevRight, size: 16 }
                    }
                }
            }
        }
    }
}

/// Tonight's set (or the next one you play), its room, call and rig.
#[component]
fn NextUp(face: Face) -> Element {
    let Some(d) = face.day() else {
        return rsx! {};
    };
    let kicker = if d.from_today() == 0 {
        "Tonight".to_string()
    } else {
        DAY[d.weekday()].to_string()
    };
    let kicker = if face.starts.is_empty() {
        kicker
    } else {
        format!("{kicker} · {}", face.starts)
    };
    let here = (use_app().here)();
    let org = crate::orgs::org_name(&here);
    let loaded = data::loaded_rig();
    let id = face.id.clone();
    rsx! {
        button {
            class: "nextup hn",
            style: "--h:{face.hue}",
            onclick: move |_| {
                navigator().push(Route::SetPage { id: id.clone() });
            },
            span { class: "nutop",
                if let Some(l) = data::logo(&face.series) {
                    img { class: "nulogo", src: l, alt: "" }
                } else {
                    crate::pieces::Cover { title: face.title.clone(), size: 64, hue: face.hue }
                }
                span { class: "mid",
                    span { class: "nuk", "{kicker}" }
                    span { class: "nut", "{face.title}" }
                    span { class: "nus", "{org} · {face.songs} songs" }
                }
            }
            span { class: "nuwhere",
                span { class: "nw",
                    Icon { name: I::Mappin, size: 13, w: 2.0 }
                    span {
                        b { "{face.place}" }
                    }
                }
                span { class: "nw call",
                    Icon { name: I::Clock, size: 13, w: 2.0 }
                    span {
                        small { "Call" }
                        b { "{face.call}" }
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
                span { class: "nuready", {progress_of(&face)} }
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
    // A set Drive has as a setlist file opens that file, not the sample.
    let app = use_app();
    let file = (kind == "Setlist")
        .then(|| {
            (app.drive_sets)()
                .unwrap_or_default()
                .into_iter()
                .find(|h| crate::reference::fit(&crate::setfile::name_of(&h.name), title).is_some())
        })
        .flatten();
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
                match &file {
                    Some(h) => {
                        navigator()
                            .push(Route::DriveSet {
                                space: h.space.clone(),
                                location: h.location.clone(),
                                path: h.path.split('/').map(str::to_string).collect(),
                            });
                    }
                    None => {
                        navigator().push(Route::open(open));
                    }
                }
            },
            span { class: "sc",
                Art { img, title: cover_title, hue }
            }
            span { class: "t1", "{title}" }
            span { class: "s1", "{kind}" }
        }
    }
}
