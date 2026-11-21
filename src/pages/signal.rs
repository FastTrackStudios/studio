//! Signal: every rig on one screen, the loaded one live; today's set
//! opened in your rig in one tap.

use dioxus::prelude::*;

use crate::data;
use crate::icons::{I, Icon};
use crate::pieces::{Bar, Hair, Page};
use crate::state::{Menu, use_app};

#[component]
pub fn Signal() -> Element {
    let app = use_app();
    let mine = data::my_sets();
    let next = mine.first().copied();
    let badge = next.and_then(|o| o.mine().map(|r| (o, r)));
    rsx! {
        Page {
            Bar {
                title: "Signal",
                tools: rsx! {
                    if let Some((o, r)) = badge {
                        div { class: "joined quick", style: "--h:{r.hue}",
                            button {
                                class: "qopen",
                                aria_label: "Open {o.name()} in the {r.name} rig",
                                onclick: move |_| {
                                    app.say(
                                        format!(
                                            "Opened {}, {} in the {} rig: {} songs, a preset each",
                                            o.name(),
                                            o.date.short(),
                                            r.name,
                                            o.songs.len(),
                                        ),
                                    )
                                },
                                Icon { name: r.icon, size: 16, w: 1.9 }
                                span { "{o.series.name} · {o.date.when()}" }
                            }
                            Hair {}
                            button {
                                class: "tool",
                                aria_label: "Your sets",
                                onclick: move |_| {
                                    let mut m = app.menu;
                                    m.set(Some(Menu::Mine));
                                },
                                Icon { name: I::Updown, size: 16 }
                            }
                        }
                    }
                },
            }
            div { class: "body rigs",
                for r in data::RIGS.iter() {
                    {
                        let n = mine.iter().filter(|o| o.mine().is_some_and(|m| m.name == r.name)).count();
                        let now = next.filter(|o| r.loaded && o.mine().is_some_and(|m| m.name == r.name));
                        let cls = if r.loaded { "rigrow loaded" } else { "rigrow" };
                        rsx! {
                            button {
                                key: "{r.name}",
                                class: "{cls}",
                                style: "--h:{r.hue}",
                                onclick: move |_| app.say(format!("Opened the {} rig", r.name)),
                                span { class: "rigicon", style: "--h:{r.hue}",
                                    Icon { name: r.icon, size: 26, w: 1.7 }
                                }
                                span { class: "mid",
                                    span { class: "name",
                                        "{r.name}"
                                        small { "{r.input}" }
                                    }
                                    if let Some(o) = now {
                                        span { class: "s1 now",
                                            "{o.series.name} · 1/{o.songs.len()} · {o.songs.first().map_or(\"\", |s| s.title)}"
                                        }
                                        span { class: "level", aria_label: "Input level",
                                            i { style: "width:46%" }
                                        }
                                    } else {
                                        span { class: "s1", "{r.preset}" }
                                    }
                                }
                                if r.loaded {
                                    span { class: "on", aria_label: "Loaded",
                                        span { class: "live" }
                                    }
                                } else if n > 0 {
                                    span { class: "yours",
                                        "{n} set"
                                        if n > 1 {
                                            "s"
                                        }
                                    }
                                } else {
                                    span { class: "chev",
                                        Icon { name: I::ChevRight, size: 18 }
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
