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
    let index = crate::sets::use_index();
    let me = crate::team::use_me();
    let mine = crate::sheets::my_sets(&index.faces(), me.as_deref());
    let next = mine.first().cloned();
    // Your sets are played on the rig you have loaded.
    let yours = data::loaded_rig().or_else(|| data::rig("Guitar"));
    let badge = next.clone().zip(yours);
    rsx! {
        Page {
            Bar {
                title: "Signal",
                tools: rsx! {
                    if let Some((o, r)) = badge {
                        {
                        let when = o.day().map(|d| d.when()).unwrap_or_default();
                        let title = o.title.clone();
                        let n = o.songs;
                        rsx! {
                        div { class: "joined quick", style: "--h:{r.hue}",
                            button {
                                class: "qopen",
                                aria_label: "Open {title} in the {r.name} rig",
                                onclick: {
                                    let (title, when) = (title.clone(), when.clone());
                                    move |_| {
                                        app.say(
                                            format!(
                                                "Opened {title}, {when} in the {} rig: {n} songs, a preset each",
                                                r.name,
                                            ),
                                        )
                                    }
                                },
                                Icon { name: r.icon, size: 16, w: 1.9 }
                                span { "{title} · {when}" }
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
                        }
                    }
                },
            }
            div { class: "body rigs",
                for r in data::RIGS.iter() {
                    {
                        let n = if yours.is_some_and(|y| y.name == r.name) { mine.len() } else { 0 };
                        let now = next.clone().filter(|_| r.loaded);
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
                                            "{o.title} · 1/{o.songs} · {o.first.first().cloned().unwrap_or_default()}"
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
