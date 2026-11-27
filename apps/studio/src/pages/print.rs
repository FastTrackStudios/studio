//! A set, printable: white paper, the running order with each song's key,
//! tempo, arrangement and who leads it, the notes and transitions, the
//! band. The browser's Print prints the page alone.

use dioxus::prelude::*;

use crate::pages::set::use_ctx;
use crate::setlist::ItemKind;

#[component]
pub fn PrintSet(owner: String, id: String) -> Element {
    let ctx = use_ctx(&owner, &id);
    let info = ctx.set.info();
    let entries = ctx.set.entries();
    let band = ctx.set.band();
    let mut k = 0;
    let when = info.day().map(|d| {
        format!(
            "{}, {} {} {}",
            crate::day::DAY[d.weekday()],
            crate::day::MONTH[d.month0()],
            d.mday(),
            d.year()
        )
    });
    let back = crate::pages::set_route(&owner, &id, &ctx.here);
    rsx! {
        section { class: "page printwrap",
            div { class: "printbar",
                button {
                    class: "secondary",
                    onclick: move |_| {
                        navigator().push(back.clone());
                    },
                    "Back to the set"
                }
                button {
                    class: "primary",
                    onclick: move |_| {
                        document::eval("window.print()");
                    },
                    "Print"
                }
            }
            article { class: "printpage",
                h1 { "{info.title}" }
                p { class: "pmeta",
                    if let Some(w) = when {
                        "{w}"
                    }
                    if !info.place.is_empty() {
                        " · {info.place}"
                    }
                    if !info.call.is_empty() {
                        " · call {info.call}"
                    }
                    if !info.starts.is_empty() {
                        " · starts {info.starts}"
                    }
                }
                if !band.is_empty() {
                    p { class: "pband",
                        for s in band {
                            span { key: "{s.person}",
                                b { "{s.name}" }
                                " {s.parts.join(\", \")}"
                            }
                        }
                    }
                }
                if !info.notes.is_empty() {
                    p { class: "pnotes", "{info.notes}" }
                }
                table {
                    thead {
                        tr {
                            th { "#" }
                            th { "" }
                            th { "Key" }
                            th { "BPM" }
                            th { "Arrangement" }
                            th { "Leads" }
                        }
                    }
                    tbody {
                        for e in entries {
                            {
                                let row = match e.kind {
                                    ItemKind::Section => rsx! {
                                        tr { class: "psec",
                                            td { colspan: "6", "{e.title}" }
                                        }
                                    },
                                    ItemKind::Song => {
                                        k += 1;
                                        let p = ctx.played(&e);
                                        let moved = p.own_key.as_ref().filter(|o| Some(*o) != p.key.as_ref());
                                        rsx! {
                                            tr {
                                                td { class: "pn", "{k}" }
                                                td {
                                                    b { "{e.title}" }
                                                    if let Some(a) = &e.artist {
                                                        span { class: "part2", " · {a}" }
                                                    }
                                                }
                                                td { class: "pkey",
                                                    "{p.key.clone().unwrap_or_default()}"
                                                    if let Some(o) = moved {
                                                        small { " (from {o})" }
                                                    }
                                                }
                                                td { "{p.tempo.map(|t| t.to_string()).unwrap_or_default()}" }
                                                td { if p.arrangement != "Normal" { "{p.arrangement}" } }
                                                td { "{p.leader.as_ref().map(|l| ctx.name(l)).unwrap_or_default()}" }
                                            }
                                        }
                                    }
                                    kind => rsx! {
                                        tr { class: "pitem",
                                            td {}
                                            td { colspan: "5", "{kind.label()}: {e.title}" }
                                        }
                                    },
                                };
                                rsx! {
                                    Fragment { key: "{e.id}",
                                        {row}
                                        if !e.notes.is_empty() || !e.transition.is_empty() {
                                            tr { class: "pnote",
                                                td {}
                                                td { colspan: "5",
                                                    "{e.notes}"
                                                    if !e.transition.is_empty() {
                                                        " → {e.transition}"
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
            }
        }
    }
}
