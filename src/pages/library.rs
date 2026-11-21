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

    let lib = crate::songs::use_library();
    let all_songs = lib.entries();
    let f = filters();
    let list: Vec<crate::songs::Entry> = all_songs.iter().filter(|x| x.admitted(&f)).cloned().collect();
    let within = all_songs
        .iter()
        .filter(|x| base.is_none_or(|t| x.tags.iter().any(|y| y == t)))
        .count();
    let chips = f.chips();
    let n_filters = chips.len();
    let count = if list.len() == within {
        format!("{within} songs")
    } else {
        format!("{} of {within} songs", list.len())
    };
    let title = base.map_or("All songs".to_string(), cap);
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
                NewSong { lib: lib.clone(), tag: base }
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
                            let sub: Vec<String> = [x.artist.clone(), x.tempo.map(|t| format!("{t} bpm"))]
                                .into_iter()
                                .flatten()
                                .collect();
                            let id = x.id.clone();
                            let key = x.key.as_deref().and_then(data::norm_key);
                            rsx! {
                                button {
                                    key: "{x.id}",
                                    class: "song lib",
                                    onclick: move |_| {
                                        navigator().push(Route::SongPage { id: id.clone() });
                                    },
                                    EntryArt { title: x.title.clone(), artist: x.artist.clone() }
                                    span { class: "mid",
                                        span { class: "t1", "{x.title}" }
                                        span { class: "s1", "{sub.join(\" · \")}" }
                                    }
                                    span { class: "libr",
                                        if let Some(k) = key {
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

/// A library song's art: JT's cover on his own songs, a generated one otherwise.
#[component]
fn EntryArt(title: String, artist: Option<String>) -> Element {
    let own = artist.as_deref().is_some_and(|a| data::OWN.contains(&a));
    let hue = data::str_hue(artist.as_deref().unwrap_or(&title));
    rsx! {
        span { class: "sart",
            if own {
                img { src: data::JT_COVER, alt: "" }
            } else {
                Cover { title, size: 44, hue }
            }
        }
    }
}

/// A new song: its title, then its session opens.
#[component]
fn NewSong(lib: crate::songs::Library, tag: Option<&'static str>) -> Element {
    let mut title = use_signal(String::new);
    rsx! {
        label { class: "newsong",
            Icon { name: I::Plus, size: 16 }
            input {
                placeholder: "New song",
                value: "{title}",
                aria_label: "New song title",
                oninput: move |e| title.set(e.value()),
                onkeydown: move |e: KeyboardEvent| {
                    let t = title().trim().to_string();
                    if e.key().to_string() == "Enter" && !t.is_empty() {
                        let id = session_sync::slug::slugify(&t);
                        if lib.get(&id).is_none() {
                            lib.put(&crate::songs::Entry {
                                id: id.clone(),
                                title: t,
                                artist: None,
                                key: None,
                                tempo: Some(120),
                                tags: tag.map(|t| vec![t.to_string()]).unwrap_or_default(),
                                energy: 3,
                                ready: String::new(),
                            });
                        }
                        title.set(String::new());
                        navigator().push(Route::SongPage { id });
                    }
                },
            }
        }
    }
}
