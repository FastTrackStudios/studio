//! The pages, and the rows several of them share.

pub mod home;
pub mod library;
pub mod live;
pub mod records;
pub mod set;
pub mod setlists;
pub mod signal;
pub mod song;

use dioxus::prelude::*;

use crate::data::{self, Occ, Rehearsal};
use crate::icons::{I, Icon};
use crate::pieces::{DayTile, Progress};
use crate::state::{use_app, Booking, Sheet};
use crate::Route;

/// A set in an agenda: its day, its logo, where and when the band is called.
#[component]
pub fn OccRow(id: String, #[props(default = VNode::empty())] state: Element) -> Element {
    let Some(o) = data::occ(&id) else { return rsx! {} };
    let pl = o.series.place();
    rsx! {
        button {
            class: "ag",
            onclick: move |_| {
                navigator().push(Route::set(&id));
            },
            DayTile { day: o.date }
            if let Some(l) = data::logo(o.series.id) {
                img { class: "aglogo", src: l, alt: "" }
            }
            span { class: "mid",
                span { class: "t1", "{o.series.name}" }
                span { class: "s1",
                    Icon { name: I::Mappin, size: 11, w: 2.0 }
                    "{pl.map_or(\"\", |p| p.room)}"
                    if let Some(p) = pl {
                        " · "
                        b { class: "agcall", "Call {p.call}" }
                    }
                }
            }
            {state}
        }
    }
}

/// A rehearsal in an agenda.
#[component]
pub fn RehRow(r: Rehearsal) -> Element {
    let name = data::occ(&r.set).map_or("", |o| o.series.name);
    let set = r.set.clone();
    rsx! {
        button {
            class: "ag reh",
            onclick: move |_| {
                navigator().push(Route::set(&set));
            },
            DayTile { day: r.date }
            span { class: "rehic",
                Icon { name: I::Mic, size: 17, w: 2.0 }
            }
            span { class: "mid",
                span { class: "t1",
                    "Rehearsal "
                    span { class: "rfor", "· {name}" }
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
pub fn progress_of(o: &Occ) -> Element {
    rsx! {
        Progress { n: o.songs.len(), of: o.series.size }
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

/// Book a rehearsal for a given set.
pub fn book_for(app: crate::state::AppState, o: &Occ) {
    let mut b = app.booking;
    b.set(Some(Booking {
        set: o.id.clone(),
        day: o.date.from_today().max(1) as usize - 1,
        time: "7:00 PM",
        room: o.series.place().map_or("Room 210", |p| p.room),
    }));
    app.open(Sheet::Book);
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
                span { class: "omark", style: "--h:{data::org_hue(o)}",
                    "{data::org_name(o).chars().next().unwrap_or('?')}"
                }
                "{data::org_name(o)}"
            } else {
                Icon { name: I::Layers, size: 16 }
                "All orgs"
            }
            Icon { name: I::Updown, size: 14 }
        }
    }
}
