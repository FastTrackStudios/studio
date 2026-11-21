//! Records: what is made in the studio. A shelf of covers; inside a
//! record its tracks by status; a track's page is its review: versions,
//! the waveform with its comments, the playhead.

use dioxus::prelude::*;

use super::home::TrackCover;
use crate::Route;
use crate::data::{self, Comment, dur};
use crate::icons::{I, Icon};
use crate::pieces::{Art, Avatar, Bar, Hair, Hero, Joined, Page, StageTag, Tool};
use crate::state::{Menu, Review, Sheet, use_app};

#[component]
pub fn Records() -> Element {
    let app = use_app();
    let org = (app.org)();
    let ids: Vec<&'static str> = data::RECENT_WORK
        .iter()
        .copied()
        .filter(|id| org.is_none_or(|o| data::owner_of(id) == o))
        .collect();
    rsx! {
        Page {
            Bar {
                title: "Records",
                tools: rsx! {
                    Joined {
                        Tool {
                            label: "Search",
                            icon: I::Search,
                            onclick: move |_| app.open(Sheet::Palette),
                        }
                        Hair {}
                        Tool {
                            label: "New record",
                            icon: I::Plus,
                            onclick: move |_| app.say("New album, EP or single"),
                        }
                    }
                },
            }
            div { class: "body",
                div { class: "lgrid2",
                    for id in ids {
                        RecordTile { key: "{id}", id }
                    }
                }
            }
        }
    }
}

/// A record (or a single) as its cover, its status and what is open on it.
#[component]
fn RecordTile(id: &'static str) -> Element {
    let app = use_app();
    let (title, artist, hue, st, what, n) = if let Some(a) = data::album(id) {
        let n: usize = data::STUDIO
            .iter()
            .filter(|t| t.rec == Some(id))
            .map(|t| app.open_on(t.title))
            .sum();
        (
            a.title,
            a.artist,
            a.hue,
            app.status_of(a),
            format!("{} tracks", a.tracks.len()),
            n,
        )
    } else if let Some(t) = id.strip_prefix("t:").and_then(data::track) {
        let what = if t.cover { "Cover single" } else { "Single" };
        (
            t.title,
            t.artist,
            t.hue,
            app.stage_of(t),
            what.to_string(),
            app.open_on(t.title),
        )
    } else {
        return rsx! {};
    };
    rsx! {
        button {
            class: "lg",
            onclick: move |_| {
                navigator().push(Route::open(id));
            },
            span { class: "lgc",
                Art { img: data::art(id), title, hue }
                if n > 0 {
                    span { class: "rbub",
                        Icon { name: I::Message, size: 11, w: 2.4 }
                        "{n}"
                    }
                }
            }
            span { class: "lgt",
                span { class: "t1", "{title}" }
                span { class: "s1", "{artist}" }
                span { class: "lgs",
                    StageTag { stage: st }
                    span { "{what}" }
                }
            }
        }
    }
}

/// Inside a record: its tracks, each with its latest version and status.
#[component]
pub fn Album(id: String) -> Element {
    let app = use_app();
    let Some(a) = data::album(&id) else {
        return rsx! {
            Page { Bar { back: Route::Records {}, title: "Not found" } }
        };
    };
    let list: Vec<&'static data::Track> = data::STUDIO
        .iter()
        .filter(|t| t.rec == Some(a.id))
        .collect();
    let total: u32 = list
        .iter()
        .filter_map(|t| app.latest(t.title))
        .map(|v| v.len)
        .sum();
    let open: usize = list.iter().map(|t| app.open_on(t.title)).sum();
    let rec = format!("rec:{}", a.id);
    rsx! {
        Page { tint: a.hue,
            Bar {
                back: Route::Records {},
                tools: rsx! {
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
                                m.set(Some(Menu::Project));
                            },
                        }
                    }
                },
            }
            div { class: "body",
                Hero {
                    img: data::art(a.id),
                    hue: a.hue,
                    title: a.title,
                    kicker: a.what.to_string(),
                    facts: rsx! {},
                    sub: rsx! {
                        "{a.artist} · "
                        StageTag {
                            stage: app.status_of(a),
                            onclick: move |_| app.open(Sheet::Status(rec.clone())),
                        }
                        span { class: "hcount",
                            "{list.len()} tracks · {dur(total as f64)}"
                            if open > 0 {
                                " · "
                                span { class: "openn", "{open} open" }
                            }
                        }
                    },
                }
                div { class: "scard",
                    for t in list {
                        AlbumTrack { key: "{t.idx}", idx: t.idx }
                    }
                }
            }
        }
    }
}

#[component]
fn AlbumTrack(idx: usize) -> Element {
    let app = use_app();
    let x = &data::STUDIO[idx];
    let Some(v) = app.latest(x.title) else {
        return rsx! {};
    };
    let n = app.open_on(x.title);
    let open = move |_| {
        navigator().push(Route::TrackPage { idx });
    };
    rsx! {
        div { class: "trow roomy rec",
            span { class: "tno", "{x.no}" }
            button { class: "mid", onclick: open,
                span { class: "t1", "{x.title}" }
                span { class: "s1", "{v.who} · {v.when}" }
                span { class: "meta3",
                    if n > 0 {
                        span { class: "ld cmt",
                            Icon { name: I::Message, size: 12, w: 2.0 }
                            "{n} open"
                        }
                    } else {
                        span { class: "ld", "{dur(v.len as f64)}" }
                    }
                }
            }
            span { class: "rcol",
                button { class: "vtag", onclick: open, "{v.what} v{v.v}" }
                StageTag {
                    stage: app.stage_of(x),
                    onclick: move |_| app.open(Sheet::Status(x.title.to_string())),
                }
            }
        }
    }
}

/// A track's page: its record, then its review.
#[component]
pub fn TrackPage(idx: usize) -> Element {
    let app = use_app();
    let Some(x) = data::STUDIO.get(idx) else {
        return rsx! {
            Page { Bar { back: Route::Records {}, title: "Not found" } }
        };
    };
    let mut review = app.review;
    if review.peek().title != x.title {
        review.set(Review {
            title: x.title.into(),
            v: None,
            pos: 0.18,
            playing: false,
            draft: String::new(),
        });
    }
    let kicker = match x.rec.and_then(data::album) {
        Some(a) => format!("{} · Track {}", a.title, x.no),
        None if x.cover => "Cover single".into(),
        None => "Single".into(),
    };
    let back = x
        .rec
        .map_or(Route::Records {}, |r| Route::Album { id: r.into() });
    let say = format!("Opened {} in Session", x.title);
    rsx! {
        Page {
            Bar {
                back,
                tools: rsx! {
                    Joined {
                        Tool {
                            label: "Open in Session",
                            icon: I::Open,
                            onclick: move |_| app.say(say.clone()),
                        }
                        Hair {}
                        Tool {
                            label: "More",
                            icon: I::More,
                            onclick: move |_| {
                                let mut m = app.menu;
                                m.set(Some(Menu::Project));
                            },
                        }
                    }
                },
            }
            div { class: "body",
                div { class: "trkhead",
                    TrackCover { idx, size: 56 }
                    span { class: "mid",
                        span { class: "hk", "{kicker}" }
                        span { class: "t", "{x.title}" }
                        span { class: "s1", "{x.artist}" }
                    }
                }
                ReviewBody { idx }
            }
        }
    }
}

/// A waveform, the same for the same seed: bar heights 0–1.
fn wave_bars(seed: usize, n: usize) -> Vec<f64> {
    let mut x: u64 = seed as u64 * 9301 + 49297;
    (0..n)
        .map(|k| {
            x = (x * 9301 + 49297) % 233280;
            let r = x as f64 / 233280.0;
            0.2 + 0.8 * ((k as f64 / 6.0 + seed as f64).sin()).abs() * (0.5 + 0.5 * r)
        })
        .collect()
}

/// Reviewing a track: pick a version, hear it, comment at the playhead.
#[component]
fn ReviewBody(idx: usize) -> Element {
    let app = use_app();
    let mut wave_el = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let x = &data::STUDIO[idx];
    let mut review = app.review;
    let mut versions = app.versions;
    let r = review();
    let vs = versions.read().get(x.title).cloned().unwrap_or_default();
    let Some(v) = vs.iter().find(|y| Some(y.v) == r.v).or(vs.last()).cloned() else {
        return rsx! {};
    };
    let mut cs: Vec<(usize, Comment)> = v.comments.iter().cloned().enumerate().collect();
    cs.sort_by(|a, b| a.1.at.total_cmp(&b.1.at));
    let n_open = cs.iter().filter(|c| c.1.open).count();
    let head = if cs.is_empty() {
        "No comments on this version".to_string()
    } else {
        let s = if cs.len() > 1 { "s" } else { "" };
        let o = if n_open > 0 {
            format!(" · {n_open} open")
        } else {
            String::new()
        };
        format!("{} comment{s}{o}", cs.len())
    };
    let bars = wave_bars(x.title.len(), 72);
    let nb = bars.len();
    let pos = r.pos;
    let at = dur(v.len as f64 * pos);
    let vnum = v.v;
    let title = x.title;
    let len = v.len;
    rsx! {
        div { class: "vpick",
            for y in vs.iter() {
                button {
                    key: "{y.v}",
                    class: if y.v == v.v { "vchip on" } else { "vchip" },
                    onclick: {
                        let yv = y.v;
                        move |_| review.with_mut(|r| r.v = Some(yv))
                    },
                    "{y.what} v{y.v}"
                    if y.comments.iter().any(|c| c.open) {
                        i {}
                    }
                }
            }
            if vs.len() > 1 {
                button {
                    class: "vchip ab",
                    onclick: move |_| app.say(format!("A/B: {} v{} against v{}", v.what, vnum, vnum - 1)),
                    "A/B"
                }
            }
        }
        div { class: "vmeta",
            Avatar { who: v.who }
            span { "{v.who} · {v.when}" }
            if v.approved {
                span { class: "appr",
                    Icon { name: I::Check, size: 12, w: 2.6 }
                    "Approved"
                }
            } else {
                StageTag {
                    stage: app.stage_of(x),
                    onclick: move |_| app.open(Sheet::Status(title.to_string())),
                }
            }
        }
        div {
            class: "bwave",
            onmounted: move |e| wave_el.set(Some(e.data())),
            onclick: move |e: MouseEvent| {
                let x = e.element_coordinates().x;
                spawn(async move {
                    let Some(el) = wave_el() else { return };
                    if let Ok(rect) = el.get_client_rect().await && rect.width() > 0.0 {
                        review.with_mut(|r| r.pos = (x / rect.width()).clamp(0.0, 1.0));
                    }
                });
            },
            for (k , h) in bars.iter().enumerate() {
                i {
                    key: "{k}",
                    class: if (k as f64 / nb as f64) < pos { "pl" },
                    style: "height:{(h * 100.0).round()}%",
                }
            }
            span { class: "ph", style: "left:{pos * 100.0}%" }
            for (k , c) in v.comments.iter().enumerate() {
                button {
                    key: "{k}",
                    class: if c.open { "bpin" } else { "bpin gone" },
                    style: "left:{c.at * 100.0}%;--h:{data::person_hue(c.who)}",
                    aria_label: "{c.who} at {c.t}",
                    onclick: {
                        let at = c.at;
                        move |e: MouseEvent| {
                            e.stop_propagation();
                            review.with_mut(|r| r.pos = at)
                        }
                    },
                    "{c.who.chars().next().unwrap_or('?')}"
                }
            }
        }
        div { class: "rtrans",
            span { class: "rt", "{at}" }
            button {
                class: "rplay",
                aria_label: if r.playing { "Pause" } else { "Play" },
                onclick: move |_| review.with_mut(|r| r.playing = !r.playing),
                Icon { name: if r.playing { I::Pause } else { I::Play }, size: 22, w: 2.0 }
            }
            span { class: "rt", "{dur(len as f64)}" }
        }
        div { class: "dayhead", "{head}" }
        div { class: "clist",
            for (k , c) in cs {
                div { key: "{k}", class: if c.open { "cm2" } else { "cm2 resolved" },
                    button {
                        class: "tstamp",
                        onclick: {
                            let at = c.at;
                            move |_| review.with_mut(|r| r.pos = at)
                        },
                        "{c.t}"
                    }
                    span { class: "mid",
                        span { class: "s1",
                            b { "{c.who}" }
                        }
                        span { class: "tx", "{c.text}" }
                    }
                    if c.open {
                        button {
                            class: "rsv",
                            aria_label: "Resolve",
                            onclick: move |_| {
                                versions
                                    .with_mut(|m| {
                                        if let Some(c) = m
                                            .get_mut(title)
                                            .and_then(|vs| vs.iter_mut().find(|y| y.v == vnum))
                                            .and_then(|y| y.comments.get_mut(k))
                                        {
                                            c.open = false;
                                        }
                                    })
                            },
                            Icon { name: I::Check, size: 14, w: 2.4 }
                        }
                    } else {
                        span { class: "rsvd",
                            Icon { name: I::Check, size: 13, w: 2.4 }
                        }
                    }
                }
            }
        }
        form {
            class: "composer",
            onsubmit: move |e: FormEvent| {
                e.prevent_default();
                let text = review.peek().draft.trim().to_string();
                if text.is_empty() {
                    return;
                }
                let pos = review.peek().pos;
                versions
                    .with_mut(|m| {
                        if let Some(y) = m
                            .get_mut(title)
                            .and_then(|vs| vs.iter_mut().find(|y| y.v == vnum))
                        {
                            y.comments
                                .push(Comment {
                                    at: pos,
                                    t: dur(len as f64 * pos),
                                    who: data::ME,
                                    text,
                                    open: true,
                                });
                        }
                    });
                review.with_mut(|r| r.draft.clear());
            },
            span { class: "at", "{at}" }
            input {
                placeholder: "Comment at {at}…",
                value: "{r.draft}",
                aria_label: "Comment",
                oninput: move |e| review.with_mut(|r| r.draft = e.value()),
            }
            button { class: "send", aria_label: "Post",
                Icon { name: I::ArrowUp, size: 16, w: 2.4 }
            }
        }
    }
}
