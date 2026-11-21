//! A set: a week's set (its Top and Bottom Set, who leads, the key and
//! arrangement each song is played in) or a show (its acts).

use dioxus::prelude::*;

use crate::data::{self, Song, ME};
use crate::icons::{I, Icon};
use crate::pieces::{Bar, Cal, Hair, Hero, Joined, Page, SongArt, Tool};
use crate::state::{use_app, Menu, Sheet};
use crate::Route;

#[component]
pub fn SetPage(id: String) -> Element {
    let app = use_app();
    use_hook({
        let id = id.clone();
        move || app.enter_set(&id)
    });
    let tools = rsx! {
        Joined {
            Tool {
                label: "Download",
                icon: I::Download,
                onclick: move |_| app.say("Saved for offline"),
            }
            Hair {}
            Tool {
                label: "More",
                icon: I::More,
                onclick: move |_| {
                    let mut m = app.menu;
                    m.set(Some(Menu::Occ));
                },
            }
        }
    };
    if data::occ(&id).is_some() {
        rsx! {
            WeekSet { id, tools }
        }
    } else if data::show(&id).is_some() {
        rsx! {
            ShowSet { id, tools }
        }
    } else {
        rsx! {
            Page { Bar { back: Route::Setlists {}, title: "Not found" } }
        }
    }
}

/// A week's set, in its sections.
#[component]
fn WeekSet(id: String, tools: Element) -> Element {
    let Some(o) = data::occ(&id) else { return rsx! {} };
    let sr = o.series;
    let pl = sr.place();
    let folder = sr.folder().map_or("", |f| f.name);
    let short_of = o.songs.len() < sr.size;
    let total = o.total();
    let missing = sr.size.saturating_sub(o.songs.len());
    let rows = o.songs.len() + missing;
    rsx! {
        Page { tint: sr.hue,
            Bar { back: Route::Series { id: sr.id.into() }, tools }
            div { class: "body",
                Hero {
                    img: data::logo(sr.id),
                    hue: sr.hue,
                    title: sr.name,
                    facts: rsx! {
                        span { class: "when2",
                            Cal { top: data::MON[o.date.month0()], big: o.date.mday().to_string() }
                            span { class: "loc",
                                Icon { name: I::Mappin, size: 14, w: 2.0 }
                                span {
                                    b { "{pl.map_or(\"\", |p| p.room)}" }
                                    if let Some(p) = pl {
                                        small {
                                            span { class: "call", "Call {p.call}" }
                                            " · Starts {p.starts}"
                                        }
                                    }
                                }
                            }
                        }
                    },
                    sub: rsx! {
                        "{folder}"
                        if short_of {
                            " · "
                            span { class: "todo2", "{o.ready()}" }
                        }
                        span { class: "hcount",
                            "{o.songs.len()} songs"
                            if total > 0 {
                                " · {data::dur(total as f64)}"
                            }
                        }
                    },
                }
                for (name , from , to) in data::sections_of(o) {
                    Fragment { key: "{name}",
                    div { class: "ssec",
                        span { "{name}" }
                        span {}
                    }
                    div { class: "scard",
                        for i in from..to.min(rows) {
                            if i < o.songs.len() {
                                SetSong { key: "{i}", set: id.clone(), i }
                            } else {
                                button {
                                    key: "{i}",
                                    class: "trow roomy add",
                                    onclick: move |_| {
                                        navigator().push(Route::LibraryHome {});
                                    },
                                    span { class: "sart empty",
                                        Icon { name: I::Plus, size: 18, w: 2.0 }
                                    }
                                    span { class: "mid",
                                        span { class: "t1", "Add a song" }
                                        span { class: "s1", "From the Library" }
                                    }
                                    span {}
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

/// A song in a week's set: who leads it, its key and its arrangement.
#[component]
fn SetSong(set: String, i: usize) -> Element {
    let app = use_app();
    let Some(o) = data::occ(&set) else { return rsx! {} };
    let x = o.songs[i];
    let (v, g) = data::leads_of(o, i, &x);
    let cur = app.key_in(&set, i, &x);
    let arr = app.arr_in(o, i);
    let list = o.songs.clone();
    let from = format!("{} · {}", o.series.name, o.date.short());
    let key_set = set.clone();
    let arr_set = set.clone();
    rsx! {
        div { class: "trow roomy",
            SongArt { song: x }
            button {
                class: "mid",
                onclick: move |_| app.play(list.clone(), from.clone(), x.title),
                span { class: "t1", "{x.title}" }
                span { class: "s1", "{x.artist.unwrap_or(\"\")}" }
                span { class: "meta3",
                    if let Some(v) = v {
                        Lead { who: v, role: I::Mic }
                    }
                    if let Some(g) = g {
                        Lead { who: g, role: I::Guitar }
                    }
                }
            }
            span { class: "rcol",
                if let Some(cur) = cur {
                    KeyChip {
                        orig: x.key.unwrap_or(cur),
                        cur,
                        onclick: move |_| app.open(Sheet::Key(key_set.clone(), i)),
                    }
                }
                span {
                    class: if arr == "Normal" { "arrsm none" } else { "arrsm" },
                    role: "button",
                    aria_label: "Arrangement: {arr}",
                    onclick: move |_| app.open(Sheet::Arr(arr_set.clone(), i)),
                    Icon { name: I::Branch, size: 11, w: 2.4 }
                    if arr != "Normal" {
                        "{arr}"
                    }
                }
            }
        }
    }
}

/// Who leads: a mic or a guitar and a name; you in your rig's colour.
#[component]
fn Lead(who: &'static str, role: I) -> Element {
    if who == ME {
        let rig = if role == I::Mic { "Keys" } else { "Guitar" };
        let h = data::rig(rig).map_or(30, |r| r.hue);
        return rsx! {
            span { class: "ld me", style: "--h:{h}",
                Icon { name: role, size: 13, w: 2.0 }
                "You"
            }
        };
    }
    rsx! {
        span { class: "ld",
            Icon { name: role, size: 13, w: 2.0 }
            "{who}"
        }
    }
}

/// The key: big in its colour; moved, how far from the original.
#[component]
pub fn KeyChip(orig: &'static str, cur: &'static str, onclick: EventHandler<MouseEvent>) -> Element {
    let d = if orig != cur { data::steps(orig, cur) } else { 0 };
    let h = data::hue(cur).unwrap_or(0);
    let label = if d != 0 {
        format!(
            "Key {cur}, {} {} from {orig}",
            d.abs(),
            if d > 0 { "up" } else { "down" }
        )
    } else {
        format!("Key {cur}, the original key")
    };
    let sign = if d > 0 { "+" } else { "−" };
    rsx! {
        button {
            class: if d != 0 { "key2 moved" } else { "key2" },
            style: "--h:{h}",
            aria_label: "{label}",
            onclick: move |e| onclick.call(e),
            b { "{cur}" }
            if d != 0 {
                small { "{sign}{d.abs()}" }
            }
        }
    }
}

/// A show: its acts, each song with its key.
#[component]
fn ShowSet(id: String, tools: Element) -> Element {
    let Some(st) = data::show(&id) else { return rsx! {} };
    let folder = data::show_folder(&id).map_or("", |f| f.name);
    let n = st.songs().len();
    let total = st.total();
    rsx! {
        Page { tint: st.hue,
            Bar { back: Route::Setlists {}, tools }
            div { class: "body",
                Hero {
                    img: data::art(&id),
                    hue: st.hue,
                    title: st.title,
                    facts: rsx! {
                        Cal { top: st.cal.0, big: st.cal.1 }
                    },
                    sub: rsx! {
                        "{folder}"
                        span { class: "hcount",
                            "{n} songs"
                            if total > 0 {
                                " · {data::dur(total as f64)}"
                            }
                        }
                    },
                }
                for (name , list) in st.acts.iter() {
                    Fragment { key: "{name}",
                        div { class: "ssec",
                            "{name}"
                            span {
                                "{list.len()} songs"
                                if list.iter().any(|s| s.len.is_some()) {
                                    " · {data::dur(list.iter().filter_map(|s| s.len).sum::<u32>() as f64)}"
                                }
                            }
                        }
                        div { class: "scard",
                            for x in list.iter().copied() {
                                ShowSong {
                                    key: "{x.title}",
                                    song: x,
                                    list: st.songs(),
                                    from: st.title,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ShowSong(song: Song, list: Vec<Song>, from: &'static str) -> Element {
    let app = use_app();
    let x = song;
    let sub = match x.len {
        Some(l) => format!("{} · {}", x.artist.unwrap_or("No artist yet"), data::dur(l as f64)),
        None => x.artist.unwrap_or("No artist yet").to_string(),
    };
    rsx! {
        div { class: "trow roomy show2",
            SongArt { song: x }
            button {
                class: "mid",
                onclick: move |_| app.play(list.clone(), from.to_string(), x.title),
                span { class: "t1", "{x.title}" }
                span { class: "s1", "{sub}" }
            }
            span { class: "rcol",
                if let Some(k) = x.key {
                    span { class: "key2", style: "--h:{data::hue(k).unwrap_or(0)}",
                        b { "{k}" }
                    }
                } else {
                    span { class: "key2 nokey",
                        b { "Set key" }
                    }
                }
            }
        }
    }
}
