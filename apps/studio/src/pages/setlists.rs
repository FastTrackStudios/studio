//! Setlists: the org's sets, from its index document. Its recurring
//! events as their logos, each ringed by how ready its next set is; what
//! is coming up, what other orgs share with it, its templates and what
//! was played before. A recurring event's page, and every set there is.

use dioxus::prelude::*;

use super::{FaceRow, progress_of, set_route};
use crate::Route;
use crate::data::{self, Day, MONTH, TODAY};
use crate::icons::{I, Icon};
use crate::orgs;
use crate::pieces::{Bar, Cover, DayTile, Hair, Hero, Joined, Page, Ring, Tool};
use crate::setlist::Series as Recurring;
use crate::setlist::{self, Face, Kind};
use crate::sets;
use crate::state::{Menu, Sheet, use_app};

#[component]
pub fn Setlists() -> Element {
    let app = use_app();
    let here = (app.here)();
    let index = sets::use_index();
    let faces = index.faces();
    let series = index.series();
    let dated: Vec<&Face> = faces.iter().filter(|f| f.kind != Kind::Template).collect();
    let mut upcoming: Vec<Face> = dated
        .iter()
        .filter(|f| f.upcoming())
        .filter(|f| {
            // A recurring event's later meetings wait on its own page.
            f.series.is_empty() || f.day().is_some_and(|d| d.from_today() < 8)
        })
        .map(|f| (*f).clone())
        .collect();
    upcoming.sort_by(|a, b| a.date.cmp(&b.date));
    let mut past: Vec<Face> = dated
        .iter()
        .filter(|f| f.day().is_some_and(|d| d.from_today() < 0))
        .map(|f| (*f).clone())
        .collect();
    past.sort_by(|a, b| b.date.cmp(&a.date));
    past.truncate(6);
    let templates = index.templates();
    let shared = sets::use_sets_shared_with(&here);
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
                            label: "Every setlist",
                            icon: I::More,
                            onclick: move |_| {
                                let mut m = app.menu;
                                m.set(Some(Menu::SetMenu));
                            },
                        }
                    }
                    Tool {
                        label: "New setlist",
                        icon: I::Plus,
                        onclick: move |_| app.open(Sheet::NewSet(None)),
                    }
                },
            }
            div { class: "body",
                if !series.is_empty() {
                    div { class: "dayhead dh2",
                        span { "Recurring" }
                        button {
                            class: "seeall",
                            onclick: move |_| {
                                navigator().push(Route::Every {});
                            },
                            "Every setlist "
                            Icon { name: I::ChevRight, size: 14 }
                        }
                    }
                    div { class: "evgrid",
                        for sr in sorted_series(&series, &faces) {
                            EventTile { key: "{sr.id}", sr: sr.clone(), next: next_meeting(&faces, &sr) }
                        }
                    }
                }
                div { class: "dayhead", "Coming up" }
                if upcoming.is_empty() {
                    button {
                        class: "ag",
                        onclick: move |_| app.open(Sheet::NewSet(None)),
                        span { class: "cal tpl",
                            Icon { name: I::Plus, size: 18 }
                        }
                        span { class: "mid",
                            span { class: "t1", "Plan a set" }
                            span { class: "s1", "A service, a rehearsal, a show" }
                        }
                    }
                }
                div { class: "agenda",
                    for f in upcoming {
                        FaceRow { key: "{f.id}", face: f.clone(), state: progress_of(&f) }
                    }
                }
                FromDrive { space: here.clone() }
                if !shared().is_empty() {
                    div { class: "dayhead", "Shared with {orgs::org_name(&here)}" }
                    div { class: "agenda",
                        for (owner , id) in shared() {
                            SharedSetRow { key: "{owner}/{id}", owner, id }
                        }
                    }
                }
                if !templates.is_empty() {
                    div { class: "dayhead", "Templates" }
                    div { class: "agenda",
                        for f in templates {
                            FaceRow { key: "{f.id}", face: f.clone() }
                        }
                    }
                }
                if !past.is_empty() {
                    div { class: "dayhead", "Before" }
                    div { class: "agenda past",
                        for f in past {
                            FaceRow {
                                key: "{f.id}",
                                face: f.clone(),
                                state: rsx! {
                                    span { class: "chev",
                                        Icon { name: I::ChevRight, size: 16 }
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The setlist files in the org's Drive (`.setlist`, beside their songs'
/// session folders), each opening as a set.
#[cfg(feature = "web")]
#[component]
fn FromDrive(space: String) -> Element {
    let files = use_resource(use_reactive!(|space| async move {
        crate::drive_sets::files(&space).await
    }));
    let list = files().unwrap_or_default();
    if list.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "dayhead", "In Drive" }
        div { class: "agenda",
            for h in list {
                button {
                    key: "{h.location}/{h.path}",
                    class: "ag",
                    onclick: {
                        let h = h.clone();
                        move |_| {
                            navigator()
                                .push(Route::DriveSet {
                                    space: h.space.clone(),
                                    location: h.location.clone(),
                                    path: h.path.split('/').map(str::to_string).collect(),
                                });
                        }
                    },
                    span { class: "cal tpl",
                        Icon { name: I::Setlist, size: 18 }
                    }
                    span { class: "mid",
                        span { class: "t1", "{crate::setfile::name_of(&h.name)}" }
                        span { class: "s1", "{h.location_name} · {h.device_name}" }
                    }
                    span { class: "chev",
                        Icon { name: I::ChevRight, size: 16 }
                    }
                }
            }
        }
    }
}

#[cfg(not(feature = "web"))]
#[component]
fn FromDrive(space: String) -> Element {
    let _ = space;
    rsx! {}
}

/// A set another org shares with this one: read from its own document
/// (the owner's index is the owner's).
#[component]
fn SharedSetRow(owner: String, id: String) -> Element {
    let set = sets::use_set(&owner, &id);
    let face = setlist::face_of(&set.loro());
    if face.title.is_empty() {
        return rsx! {};
    }
    rsx! {
        FaceRow {
            face: face.clone(),
            owner: owner.clone(),
            state: rsx! {
                span { class: "orgtag", style: "--h:{orgs::org_hue(&owner)}", "{orgs::org_name(&owner)}" }
            },
        }
    }
}

/// The next meeting of a recurring event from today: its set's face, or
/// the day with no set yet.
fn next_meeting(faces: &[Face], sr: &Recurring) -> (Day, Option<Face>) {
    let day = sr.next_from(TODAY).unwrap_or(TODAY);
    let id = setlist::meeting_id(&sr.id, day);
    (day, faces.iter().find(|f| f.id == id).cloned())
}

/// The recurring events, the soonest next meeting first.
fn sorted_series(list: &[Recurring], faces: &[Face]) -> Vec<Recurring> {
    let mut v = list.to_vec();
    v.sort_by_key(|sr| next_meeting(faces, sr).0);
    v
}

/// A recurring event: its logo ringed by how ready its next set is.
#[component]
fn EventTile(sr: Recurring, next: (Day, Option<Face>)) -> Element {
    let (day, face) = next;
    let n = face.as_ref().map_or(0, |f| f.songs);
    let of = sr.size.max(1);
    let d = day.from_today();
    let when = if d == 0 {
        "Today".to_string()
    } else if d < 7 {
        day.dow3().to_string()
    } else {
        day.short()
    };
    let cls = if n >= of {
        "ok"
    } else if n > 0 {
        ""
    } else {
        "none"
    };
    let id = sr.id.clone();
    rsx! {
        button {
            class: "ev",
            onclick: move |_| {
                navigator().push(Route::Series { id: id.clone() });
            },
            span { class: "evlogo",
                Ring { n, of, size: 84 }
                if let Some(l) = data::logo(&sr.id) {
                    img { src: l, alt: "" }
                } else {
                    Cover { title: sr.name.clone(), size: 64, hue: sr.hue }
                }
            }
            span { class: "t1", "{sr.name}" }
            span { class: "s1",
                "{when} · "
                b { class: "{cls}", "{n}/{of}" }
            }
        }
    }
}

/// A recurring event: when and where it meets, its sets to come and before.
#[component]
pub fn Series(id: String) -> Element {
    let app = use_app();
    let index = sets::use_index();
    let Some(sr) = index.series_of(&id) else {
        return rsx! {
            Page {
                Bar {
                    back: Route::Setlists {},
                    title: if index.ready() { "Not found" } else { "Loading…" },
                }
            }
        };
    };
    let faces = index.faces();
    let have = setlist::meetings(&faces, &sr.id);
    // Its meetings from today for four weeks, planned or not.
    let mut up: Vec<(Day, Option<Face>)> = sr
        .dates(TODAY, TODAY.plus(27))
        .into_iter()
        .map(|d| {
            let mid = setlist::meeting_id(&sr.id, d);
            (d, have.iter().find(|f| f.id == mid).map(|f| (*f).clone()))
        })
        .collect();
    // Any planned further out, too.
    for f in have
        .iter()
        .filter(|f| f.day().is_some_and(|d| d.from_today() > 27))
    {
        if let Some(d) = f.day() {
            up.push((d, Some((*f).clone())));
        }
    }
    let mut past: Vec<Face> = have
        .iter()
        .filter(|f| f.day().is_some_and(|d| d.from_today() < 0))
        .map(|f| (*f).clone())
        .collect();
    past.reverse();
    let past_n = past.len();
    let sid = sr.id.clone();
    rsx! {
        Page { tint: sr.hue,
            Bar {
                back: Route::Setlists {},
                tools: rsx! {
                    Joined {
                        Tool {
                            label: "Plan a set",
                            icon: I::Plus,
                            onclick: move |_| app.open(Sheet::NewSet(Some(sid.clone()))),
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
                    img: data::logo(&sr.id),
                    hue: sr.hue,
                    title: sr.name.clone(),
                    facts: rsx! {
                        span { class: "when2",
                            span { class: "loc",
                                Icon { name: I::Mappin, size: 14, w: 2.0 }
                                span {
                                    b { "{sr.room}" }
                                    small {
                                        "{sr.every()}"
                                        if !sr.call.is_empty() {
                                            " · "
                                            span { class: "call", "Call {sr.call}" }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    sub: rsx! {
                        "{orgs::org_name(&index.org)}"
                        span { class: "hcount", "{sr.size} songs a set · {past_n} sets so far" }
                    },
                }
                div { class: "dayhead", "Coming up" }
                div { class: "agenda",
                    for (d , f) in up {
                        MeetingRow { key: "{d.0}", sr: sr.clone(), day: d, face: f }
                    }
                }
                div { class: "dayhead", "Before" }
                div { class: "agenda",
                    for f in past {
                        MeetingRow { key: "{f.id}", sr: sr.clone(), day: f.day().unwrap_or(TODAY), face: Some(f.clone()) }
                    }
                }
            }
        }
    }
}

/// A meeting of a recurring event: its set, or a day to plan one for.
#[component]
fn MeetingRow(sr: Recurring, day: Day, face: Option<Face>) -> Element {
    let app = use_app();
    let here = (app.here)();
    let id = setlist::meeting_id(&sr.id, day);
    let planned = face.is_some();
    let names = face.as_ref().map(|f| f.first.clone()).unwrap_or_default();
    let open = {
        let (sr, id, here) = (sr.clone(), id.clone(), here.clone());
        move |_| {
            if !planned {
                app.open(Sheet::NewSet(Some(format!("{}@{}", sr.id, day.iso()))));
                return;
            }
            navigator().push(set_route(&here, &id, &here));
        }
    };
    rsx! {
        button { class: "ag", onclick: open,
            DayTile { day }
            span { class: "mid",
                span { class: "t1", "{sr.name_on(day)} · {day.short()}" }
                span { class: "s1",
                    if !planned {
                        "Not planned yet"
                    } else if names.is_empty() {
                        "No songs yet"
                    } else {
                        "{names.join(\", \")}"
                    }
                }
            }
            if let Some(f) = face.as_ref().filter(|_| day.from_today() >= 0) {
                {progress_of(f)}
            } else if !planned {
                span { class: "st none", "Plan" }
            } else {
                span { class: "chev",
                    Icon { name: I::ChevRight, size: 16 }
                }
            }
        }
    }
}

/// Every set the org has, by month, newest first, with a search.
#[component]
pub fn Every() -> Element {
    let app = use_app();
    let mut q = app.every_q;
    let index = sets::use_index();
    let mut rows: Vec<Face> = index
        .faces()
        .into_iter()
        .filter(|f| f.kind != Kind::Template && f.day().is_some())
        .collect();
    let n_all = rows.len();
    let needle = q().to_lowercase();
    rows.retain(|r| {
        needle.is_empty()
            || r.title.to_lowercase().contains(&needle)
            || r.first.iter().any(|t| t.to_lowercase().contains(&needle))
    });
    rows.sort_by(|a, b| b.date.cmp(&a.date));
    let mut months: Vec<(i32, usize)> = Vec::new();
    for r in &rows {
        if let Some(d) = r.day() {
            let m = (d.year(), d.month0());
            if !months.contains(&m) {
                months.push(m);
            }
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
                    Fragment { key: "{y}-{mo}",
                    div { class: "dayhead", "{MONTH[mo]} {y}" }
                    div { class: "agenda",
                        for f in rows.iter().filter(|r| r.day().is_some_and(|d| d.year() == y && d.month0() == mo)) {
                            FaceRow { key: "{f.id}", face: f.clone(), state: state_of(f) }
                        }
                    }
                    }
                }
            }
        }
    }
}

/// A row's end: how ready an upcoming set is, else a chevron.
fn state_of(f: &Face) -> Element {
    if f.upcoming() {
        return progress_of(f);
    }
    rsx! {
        span { class: "chev",
            Icon { name: I::ChevRight, size: 16 }
        }
    }
}
