//! The Library: it opens on its tags, All songs first; a tag opens its
//! songs, which narrow by more tags, a search and the filter sheet.

use std::collections::HashSet;

use dioxus::prelude::*;

use crate::Route;
use crate::icons::{I, Icon};
use crate::look::{self, cap};
use crate::pieces::{Bar, Cover, Energy, KeyC, Page, Tool};
use crate::songs::Entry;
use crate::state::{Sheet, use_app};

/// Four artists' art in a square, for a tag's tile.
#[component]
fn Mosaic(list: Vec<Entry>) -> Element {
    let mut seen = HashSet::new();
    let four: Vec<_> = list
        .iter()
        .filter(|x| seen.insert(x.artist.clone().unwrap_or_else(|| x.title.clone())))
        .take(4)
        .cloned()
        .collect();
    let gaps = 4 - four.len();
    let n = list.len();
    rsx! {
        span { class: "mosaic",
            for x in four {
                EntryArt { key: "{x.id}", title: x.title.clone(), artist: x.artist.clone() }
            }
            for k in 0..gaps {
                span { key: "g{k}", class: "sart",
                    Cover { title: format!("{n}-{k}"), size: 44, hue: look::str_hue(&format!("{n}-{k}")) }
                }
            }
        }
    }
}

/// The tags of a library's songs, most used first.
fn tags_of(list: &[Entry]) -> Vec<(String, usize)> {
    let mut tags: Vec<(String, usize)> = Vec::new();
    for e in list {
        for t in &e.tags {
            match tags.iter_mut().find(|x| &x.0 == t) {
                Some(x) => x.1 += 1,
                None => tags.push((t.clone(), 1)),
            }
        }
    }
    tags.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    tags
}

#[component]
pub fn LibraryHome() -> Element {
    let app = use_app();
    let lib = crate::songs::use_library();
    let here = (app.here)();
    let list = lib.entries();
    let tags = tags_of(&list);
    let owners = crate::team::use_owners_sharing(&here);
    let name = crate::orgs::org_name(&here);
    rsx! {
        Page {
            Bar {
                back: Route::Setlists {},
                title: "Library",
            }
            div { class: "body",
                p { class: "lede", "{name}'s songs, and the songs other orgs share with it." }
                div { class: "lgrid",
                    LibTile { tag: "all", name: "All songs", wide: true, list: list.clone() }
                    LibTile { tag: "shared", name: "Shared with us", wide: true, list: Vec::new(), note: format!("from {} orgs", owners().len()) }
                    for (t , _) in tags.iter() {
                        LibTile {
                            key: "{t}",
                            tag: t.clone(),
                            name: cap(t),
                            wide: false,
                            list: list.iter().filter(|x| x.tags.contains(t)).cloned().collect::<Vec<_>>(),
                        }
                    }
                }
                if list.is_empty() && lib.ready() {
                    p { class: "empty", "{name} has no songs of its own yet. Add one from All songs, or have another org share some." }
                }
            }
        }
    }
}

#[component]
fn LibTile(
    tag: String,
    name: String,
    wide: bool,
    list: Vec<Entry>,
    #[props(default)] note: Option<String>,
) -> Element {
    let n = list.len();
    let line = note.unwrap_or_else(|| format!("{n} songs"));
    rsx! {
        button {
            class: if wide { "ltile wide" } else { "ltile" },
            onclick: move |_| {
                navigator().push(Route::Library { tag: tag.clone() });
            },
            Mosaic { list }
            span { class: "ln1", "{name}" }
            span { class: "ln2", "{line}" }
        }
    }
}

/// What other orgs share with this one, each owner's songs together.
#[component]
fn SharedWithUs() -> Element {
    let app = use_app();
    let here = (app.here)();
    let owners = crate::team::use_owners_sharing(&here);
    let list = owners();
    rsx! {
        Page {
            Bar { back: Route::LibraryHome {}, title: "Shared with {crate::orgs::org_name(&here)}" }
            div { class: "body",
                p { class: "lede",
                    "Songs other orgs share with {crate::orgs::org_name(&here)}. Each stays its owner's; the key, arrangement and leader here are this org's own."
                }
                if list.is_empty() {
                    p { class: "empty", "No org shares songs with {crate::orgs::org_name(&here)} yet." }
                }
                for owner in list {
                    SharedFrom { key: "{owner}", owner, here: here.clone() }
                }
            }
        }
    }
}

#[component]
fn SharedFrom(owner: String, here: String) -> Element {
    let digest = crate::team::use_digest(&owner, &here);
    let lib = crate::songs::use_library();
    let list = digest.entries();
    if list.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "dayhead ownerhead",
            crate::pages::orgs::OrgMark { org: owner.clone(), size: 18 }
            "From {crate::orgs::org_name(&owner)}"
        }
        div { class: "list",
            for x in list {
                SongRow { key: "{x.id}", owner: owner.clone(), here: here.clone(), u: lib.use_of(&owner, &x.id), entry: x }
            }
        }
    }
}

/// A song in a list: its art, title and artist, its key here, its energy;
/// a shared one says by whom, one this org shares says with how many.
#[component]
fn SongRow(entry: Entry, owner: String, here: String, u: crate::orgs::Use) -> Element {
    let x = entry;
    let sub: Vec<String> = [x.artist.clone(), x.tempo.map(|t| format!("{t} bpm"))]
        .into_iter()
        .flatten()
        .collect();
    let key_here = crate::orgs::key_here(&u, &x);
    let key = key_here.as_deref().and_then(look::norm_key);
    let moved = u.key.is_some();
    let own = owner == here;
    let (id, o) = (x.id.clone(), owner.clone());
    let shared_n = x.shared_with.len();
    rsx! {
        button {
            class: "song lib",
            onclick: move |_| {
                if own {
                    navigator().push(Route::SongPage { id: id.clone() });
                } else {
                    navigator().push(Route::SharedSong { owner: o.clone(), id: id.clone() });
                }
            },
            EntryArt { title: x.title.clone(), artist: x.artist.clone() }
            span { class: "mid",
                span { class: "t1", "{x.title}" }
                span { class: "s1",
                    "{sub.join(\" · \")}"
                    if own && shared_n > 0 {
                        span { class: "sharedn", " · shared with {shared_n}" }
                    }
                }
            }
            span { class: "libr",
                if let Some(k) = key {
                    span { class: if moved { "keyhere moved" } else { "keyhere" }, KeyC { k } }
                }
                Energy { e: x.energy }
            }
        }
    }
}

#[component]
pub fn Library(tag: String) -> Element {
    if tag == "shared" {
        return rsx! {
            SharedWithUs {}
        };
    }
    rsx! {
        OwnSongs { tag }
    }
}

#[component]
fn OwnSongs(tag: String) -> Element {
    let app = use_app();
    let mut filters = app.filters;
    let lib = crate::songs::use_library();
    let here = (app.here)();
    let all_songs = lib.entries();
    let tags = tags_of(&all_songs);
    let base: Option<String> = tags.iter().map(|t| t.0.clone()).find(|t| *t == tag);

    // Opening a tag starts its filters from that tag alone.
    {
        let base = base.clone();
        use_hook(move || filters.with_mut(|f| f.tags = base.into_iter().collect()));
    }

    let f = filters();
    let list: Vec<crate::songs::Entry> = all_songs
        .iter()
        .filter(|x| x.admitted(&f))
        .cloned()
        .collect();
    let within = all_songs
        .iter()
        .filter(|x| base.as_ref().is_none_or(|t| x.tags.iter().any(|y| y == t)))
        .count();
    let chips = f.chips();
    let n_filters = chips.len();
    let count = if list.len() == within {
        format!("{within} songs")
    } else {
        format!("{} of {within} songs", list.len())
    };
    let title = base.as_deref().map_or("All songs".to_string(), cap);
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
                    for (t , _) in tags.iter().filter(|t| Some(&t.0) != base.as_ref()).cloned() {
                        button {
                            key: "{t}",
                            class: "tagchip",
                            aria_pressed: "{f.tags.contains(&t)}",
                            onclick: {
                                let t = t.clone();
                                move |_| {
                                    filters
                                        .with_mut(|f| {
                                            if !f.tags.remove(&t) {
                                                f.tags.insert(t.clone());
                                            }
                                        })
                                }
                            },
                            "{cap(&t)}"
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
                button {
                    class: "fopt",
                    aria_label: "New session: from scratch, a folder of stems or a REAPER project",
                    onclick: move |_| app.open(Sheet::NewSession),
                    Icon { name: I::Plus, size: 14 }
                    " New session…"
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
                    for x in list.iter().cloned() {
                        SongRow { key: "{x.id}", owner: here.clone(), here: here.clone(), u: lib.use_of(&here, &x.id), entry: x }
                    }
                }
            }
        }
    }
}

/// A library song's art: a cover generated from its title and artist.
#[component]
fn EntryArt(title: String, artist: Option<String>) -> Element {
    let hue = look::str_hue(artist.as_deref().unwrap_or(&title));
    rsx! {
        span { class: "sart",
            Cover { title, size: 44, hue }
        }
    }
}

/// A new song: its title, then its session opens.
#[component]
fn NewSong(lib: crate::songs::Library, tag: Option<String>) -> Element {
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
                                tags: tag.clone().map(|t| vec![t]).unwrap_or_default(),
                                energy: 3,
                                ready: String::new(),
                                shared_with: Vec::new(),
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
