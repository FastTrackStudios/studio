//! The pages, and the rows several of them share.

pub mod arrangement;
pub mod drive_set;
pub mod files;
pub mod home;
pub mod library;
pub mod live;
pub mod orgs;
pub mod print;
pub mod records;
pub mod set;
pub mod setlists;
pub mod signal;
pub mod song;

use dioxus::prelude::*;

use crate::Route;
use crate::data;
use crate::icons::{I, Icon};
use crate::pieces::{DayTile, Progress};
use crate::setlist::{Face, Info};
use crate::state::{Booking, SetRef, Sheet, use_app};

/// Where a set opens: its page in the org Studio is in, else its org's.
pub fn set_route(owner: &str, id: &str, here: &str) -> Route {
    if owner == here {
        Route::SetPage { id: id.into() }
    } else {
        Route::OrgSet {
            owner: owner.into(),
            id: id.into(),
        }
    }
}

/// A set in an agenda: its day, its logo, where and when the band is called.
#[component]
pub fn FaceRow(
    face: Face,
    #[props(default)] owner: Option<String>,
    #[props(default = VNode::empty())] state: Element,
) -> Element {
    let here = (use_app().here)();
    let owner = owner.unwrap_or_else(|| here.clone());
    let route = set_route(&owner, &face.id, &here);
    let logo = data::logo(&face.series).or_else(|| data::art(&face.id));
    rsx! {
        button {
            class: "ag",
            onclick: move |_| {
                navigator().push(route.clone());
            },
            if let Some(d) = face.day() {
                DayTile { day: d }
            } else {
                span { class: "cal tpl",
                    Icon { name: I::Layers, size: 18 }
                }
            }
            if let Some(l) = logo {
                img { class: "aglogo", src: l, alt: "" }
            } else {
                span { class: "aglogo",
                    crate::pieces::Cover { title: face.title.clone(), size: 36, hue: face.hue }
                }
            }
            span { class: "mid",
                span { class: "t1", "{face.title}" }
                span { class: "s1",
                    if !face.place.is_empty() {
                        Icon { name: I::Mappin, size: 11, w: 2.0 }
                        "{face.place}"
                    }
                    if !face.call.is_empty() {
                        " · "
                        b { class: "agcall", "Call {face.call}" }
                    }
                    if face.place.is_empty() && face.call.is_empty() {
                        if face.first.is_empty() {
                            "No songs yet"
                        } else {
                            "{face.first.join(\", \")}"
                        }
                    }
                }
            }
            {state}
        }
    }
}

/// A rehearsal in an agenda.
#[component]
pub fn RehRow(face: Face, r: crate::setlist::Rehearsal) -> Element {
    let here = (use_app().here)();
    let route = set_route(&here, &face.id, &here);
    let Some(day) = crate::setlist::parse_day(&r.date) else {
        return rsx! {};
    };
    rsx! {
        button {
            class: "ag reh",
            onclick: move |_| {
                navigator().push(route.clone());
            },
            DayTile { day }
            span { class: "rehic",
                Icon { name: I::Mic, size: 17, w: 2.0 }
            }
            span { class: "mid",
                span { class: "t1",
                    "Rehearsal "
                    span { class: "rfor", "· {face.title}" }
                }
                span { class: "s1", "{r.room} · {r.time}" }
            }
            span { class: "chev",
                Icon { name: I::ChevRight, size: 16 }
            }
        }
    }
}

/// The readiness of a set, as its ring.
pub fn progress_of(f: &Face) -> Element {
    let of = if f.size == 0 { f.songs.max(1) } else { f.size };
    rsx! {
        Progress { n: f.songs, of }
    }
}

/// The "+ Rehearsal" button over an agenda.
#[component]
pub fn BookButton() -> Element {
    let app = use_app();
    rsx! {
        button {
            class: "bookbtn",
            onclick: move |_| {
                let mut b = app.booking;
                b.set(None);
                app.open(Sheet::Book);
            },
            Icon { name: I::Plus, size: 14, w: 2.4 }
            "Rehearsal"
        }
    }
}

/// Book a rehearsal for a given set: the day before it, at seven.
pub fn book_for(app: crate::state::AppState, set: &SetRef, info: &Info) {
    let mut b = app.booking;
    let date = info.day();
    b.set(Some(Booking {
        set: set.clone(),
        title: info.title.clone(),
        date,
        day: date.map_or(1, |d| (d.from_today() - 1).max(0) as usize),
        time: "7:00 PM",
        room: if info.place.is_empty() {
            "Room 210"
        } else {
            leak_room(&info.place)
        },
    }));
    app.open(Sheet::Book);
}

/// The rooms the booking sheet offers are fixed strings; a set's own
/// place is kept to one of them when it is one.
fn leak_room(place: &str) -> &'static str {
    [
        "Room 210",
        "Chapel",
        "Worship Center",
        "The Loft",
        "Studio A",
        "Recital Hall",
    ]
    .into_iter()
    .find(|r| *r == place)
    .unwrap_or("Room 210")
}

/// The org switcher's button.
#[component]
pub fn OrgPill() -> Element {
    let app = use_app();
    let org = (app.org)();
    rsx! {
        button {
            class: "orgpill",
            aria_label: "Organization",
            onclick: move |_| app.open(Sheet::Orgs),
            if let Some(o) = org {
                span { class: "omark", style: "--h:{crate::orgs::org_hue(&o)}",
                    "{crate::orgs::org_name(&o).chars().next().unwrap_or('?')}"
                }
                "{crate::orgs::org_name(&o)}"
            } else {
                Icon { name: I::Layers, size: 16 }
                "All orgs"
            }
            Icon { name: I::Updown, size: 14 }
        }
    }
}
