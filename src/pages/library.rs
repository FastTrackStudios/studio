//! The Library: it opens on its tags, All songs first; a tag opens its
//! songs, which narrow by more tags, a search and the filter sheet.

use std::collections::HashSet;

use dioxus::prelude::*;

use crate::data::{self, cap, LibSong, LIB, TAGS};
use crate::icons::{I, Icon};
use crate::pieces::{Bar, Cover, Energy, KeyC, Page, SongArt, Tool};
use crate::state::{use_app, Sheet};
use crate::Route;

/// Four artists' art in a square, for a tag's tile.
#[component]
fn Mosaic(list: Vec<LibSong>) -> Element {
    let mut seen = HashSet::new();
    let four: Vec<_> = list
        .iter()
        .filter(|x| x.song.artist.is_some_and(|a| seen.insert(a)))
        .take(4)
        .map(|x| x.song)
        .collect();
    let gaps = 4 - four.len();
    let n = list.len();
    rsx! {
        span { class: "mosaic",
            for x in four {
                SongArt { key: "{x.title}", song: x }
            }
            for k in 0..gaps {
                span { key: "g{k}", class: "sart",
                    Cover { title: format!("{n}-{k}"), size: 44, hue: data::str_hue(&format!("{n}-{k}")) }
                }
            }
        }
    }
}

#[component]
pub fn LibraryHome() -> Element {
    let app = use_app();
    rsx! {
        Page {
            Bar {
                back: Route::Setlists {},
                title: "Library",
                tools: rsx! {
                    Tool {
                        label: "New tag",
                        icon: I::Plus,
                        onclick: move |_| app.say("New tag"),
                    }
                },
            }
            div { class: "body",
                div { class: "lgrid",
                    LibTile { tag: "all", name: "All songs", wide: true }
                    for (t , _) in TAGS.iter() {
                        LibTile { key: "{t}", tag: t, name: cap(t), wide: false }
                    }
                }
            }
        }
    }
}

#[component]
fn LibTile(tag: &'static str, name: String, wide: bool) -> Element {
    let list: Vec<LibSong> = LIB
        .iter()
        .filter(|x| tag == "all" || x.tags.contains(&tag))
        .cloned()
        .collect();
    let n = list.len();
    rsx! {
        button {
            class: if wide { "ltile wide" } else { "ltile" },
            onclick: move |_| {
                navigator().push(Route::Library { tag: tag.into() });
            },
            Mosaic { list }
            span { class: "ln1", "{name}" }
            span { class: "ln2", "{n} songs" }
        }
    }
}

#[component]
pub fn Library(tag: String) -> Element {
    let app = use_app();
    let mut filters = app.filters;
    let base: Option<&'static str> = TAGS.iter().map(|t| t.0).find(|t| *t == tag);

    // Opening a tag starts its filters from that tag alone.
    use_hook(move || filters.with_mut(|f| f.tags = base.into_iter().collect()));

    let f = filters();
    let list: Vec<LibSong> = LIB.iter().filter(|x| f.matches(x)).cloned().collect();
    let within = LIB
        .iter()
        .filter(|x| base.is_none_or(|t| x.tags.contains(&t)))
        .count();
    let chips = f.chips();
    let n_filters = chips.len();
    let count = if list.len() == within {
        format!("{within} songs")
    } else {
        format!("{} of {within} songs", list.len())
    };
    let title = base.map_or("All songs".to_string(), cap);
    let from = title.clone();
    rsx! {
        Page {
            Bar {
                back: Route::LibraryHome {},
                title: title.clone(),
                tools: rsx! {
                    Tool {
                        label: "Filter",
                        icon: I::Sliders,
                        badge: (n_filters > 0).then(|| n_filters.to_string()),
                        onclick: move |_| app.open(Sheet::Filters),
                    }
                },
            }
            div { class: "body",
                label { class: "search lib",
                    Icon { name: I::Search, size: 16 }
                    input {
                        r#type: "search",
                        placeholder: "Songs and artists",
                        value: "{f.q}",
                        aria_label: "Find a song",
                        oninput: move |e| filters.with_mut(|f| f.q = e.value()),
                    }
                }
                div { class: "tagrow",
                    span { class: "narrow", "Narrow" }
                    for (t , _) in TAGS.iter().filter(|t| Some(t.0) != base) {
                        button {
                            key: "{t}",
                            class: "tagchip",
                            aria_pressed: "{f.tags.contains(t)}",
                            onclick: move |_| {
                                filters
                                    .with_mut(|f| {
                                        if !f.tags.remove(t) {
                                            f.tags.insert(t);
                                        }
                                    })
                            },
                            "{cap(t)}"
                        }
                    }
                }
                if !chips.is_empty() {
                    div { class: "onrow",
                        for (k , label) in chips {
                            button {
                                key: "{k}",
                                class: "onchip",
                                onclick: move |_| filters.with_mut(|f| f.clear(k)),
                                "{label}"
                                Icon { name: I::X, size: 13, w: 2.4 }
                            }
                        }
                        button {
                            class: "clearall",
                            onclick: move |_| filters.with_mut(|f| f.clear("all")),
                            "Clear"
                        }
                    }
                }
                div { class: "libcount",
                    span { "{count}" }
                    span { class: "elegend",
                        "Energy "
                        Energy { e: 3 }
                    }
                }
                div { class: "list",
                    if list.is_empty() {
                        div { class: "empty",
                            "No song matches."
                            button { onclick: move |_| filters.with_mut(|f| f.clear("all")),
                                "Clear the filters"
                            }
                        }
                    }
                    for x in list.iter() {
                        {
                            let s = x.song;
                            let all: Vec<_> = list.iter().map(|y| y.song).collect();
                            let from = from.clone();
                            let sub: Vec<String> = [
                                    s.artist.map(str::to_string),
                                    s.tempo.map(|t| format!("{t} bpm")),
                                ]
                                .into_iter()
                                .flatten()
                                .collect();
                            rsx! {
                                button {
                                    key: "{s.title}",
                                    class: "song lib",
                                    onclick: move |_| app.play(all.clone(), format!("Library · {from}"), s.title),
                                    SongArt { song: s }
                                    span { class: "mid",
                                        span { class: "t1", "{s.title}" }
                                        span { class: "s1", "{sub.join(\" · \")}" }
                                    }
                                    span { class: "libr",
                                        if let Some(k) = s.key {
                                            KeyC { k }
                                        }
                                        Energy { e: x.energy }
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
