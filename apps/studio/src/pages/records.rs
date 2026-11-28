//! Records: what is made in the studio, from the record files in Studio's
//! home and Drive (`crate::records`). A shelf of covers; inside a record
//! its tracks in album order, each with whether it has a session yet.

use dioxus::prelude::*;

use crate::Route;
use crate::icons::{I, Icon};
use crate::pieces::{Art, Bar, Cover, Hero, Page, Tool};
use crate::records::{Record, TrackState};
use crate::state::use_app;

/// The records of the org Studio is in (every org's in All orgs).
pub fn of_org(records: &[Record], org: Option<&str>) -> Vec<Record> {
    records
        .iter()
        .filter(|r| org.is_none_or(|o| r.org == o))
        .cloned()
        .collect()
}

/// Where a record's file opens: its songs, read as a set's are.
pub fn open_route(r: &Record) -> Route {
    Route::DriveSet {
        space: r.file.space.clone(),
        location: r.file.location.clone(),
        path: r.file.path.split('/').map(str::to_string).collect(),
    }
}

#[component]
pub fn Records() -> Element {
    let app = use_app();
    let org = (app.org)();
    let records = (app.records)();
    let list = of_org(&records.clone().unwrap_or_default(), org.as_deref());
    rsx! {
        Page {
            Bar { title: "Records" }
            div { class: "body",
                if (app.home_stalled)() && list.is_empty() {
                    super::HomeNotAnswering {}
                } else if records.is_none() {
                    p { class: "empty", "Looking for records…" }
                } else if list.is_empty() {
                    NoRecords {}
                } else {
                    div { class: "lgrid2",
                        for r in list {
                            RecordTile { key: "{r.id}", record: r }
                        }
                    }
                }
            }
        }
    }
}

/// What the Records page and Home say when there is no record yet, and
/// how to add one.
#[component]
pub fn NoRecords() -> Element {
    rsx! {
        div { class: "empty", role: "status",
            p { "No records yet." }
            p {
                "A record is a setlist file whose first lines say "
                code { "# kind: studio" }
                ", kept in Studio's home at "
                code { "Projects/<Name>/<Name>.setlist" }
                ": one line a track, in album order (a session's id, or "
                code { "= Title" }
                " for a song with no session yet)."
            }
        }
    }
}

/// A record as its cover, and how many of its tracks have a session.
#[component]
pub fn RecordTile(record: Record) -> Element {
    let hue = crate::look::str_hue(&record.title);
    let n = record.tracks.len();
    let sessions = record.with_sessions();
    let id = record.id.clone();
    rsx! {
        button {
            class: "lg",
            onclick: move |_| {
                navigator().push(Route::Album { id: id.clone() });
            },
            span { class: "lgc",
                Art { img: None, title: record.title.clone(), hue }
            }
            span { class: "lgt",
                span { class: "t1", "{record.title}" }
                span { class: "s1", "Album" }
                span { class: "lgs",
                    span { "{n} tracks · {sessions} with a session" }
                }
            }
        }
    }
}

/// Inside a record: its tracks, in album order.
#[component]
pub fn Album(id: String) -> Element {
    let app = use_app();
    let records = (app.records)();
    let Some(r) = records
        .as_ref()
        .and_then(|v| v.iter().find(|r| r.id == id || r.title == id))
        .cloned()
    else {
        return rsx! {
            Page {
                Bar { back: Route::Records {}, title: "Records" }
                div { class: "body",
                    if (app.home_stalled)() {
                        super::HomeNotAnswering {}
                    } else if records.is_none() {
                        p { class: "empty", "Looking for records…" }
                    } else {
                        p { class: "empty", "No record {id} in Studio's home." }
                    }
                }
            }
        };
    };
    let hue = crate::look::str_hue(&r.title);
    let open = open_route(&r);
    let n = r.tracks.len();
    let sessions = r.with_sessions();
    rsx! {
        Page { tint: hue,
            Bar {
                back: Route::Records {},
                tools: rsx! {
                    Tool {
                        label: "Open",
                        icon: I::Open,
                        onclick: {
                            let open = open.clone();
                            move |_| {
                                navigator().push(open.clone());
                            }
                        },
                    }
                },
            }
            div { class: "body",
                Hero {
                    img: None,
                    hue,
                    title: r.title.clone(),
                    kicker: "Album".to_string(),
                    facts: rsx! {},
                    sub: rsx! {
                        span { class: "hcount", "{n} tracks · {sessions} with a session" }
                    },
                }
                div { class: "scard",
                    for (i , t) in r.tracks.iter().enumerate() {
                        button {
                            key: "{i}",
                            class: "trow roomy rec",
                            "data-state": t.state.slug(),
                            onclick: {
                                let open = open.clone();
                                move |_| {
                                    navigator().push(open.clone());
                                }
                            },
                            span { class: "tno", "{i + 1}" }
                            Cover { title: t.title.clone(), size: 36, hue: crate::look::str_hue(&t.title) }
                            span { class: "mid",
                                span { class: "t1", "{t.title}" }
                                span { class: "s1",
                                    if t.state == TrackState::Session {
                                        Icon { name: I::Audio, size: 12, w: 2.0 }
                                        " "
                                    }
                                    "{t.state.label()}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
