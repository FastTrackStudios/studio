//! The small pieces every page shares: the page's bar and its tools, the
//! generated covers, the date leaf, the readiness ring, key chips.

use dioxus::prelude::*;

use crate::data::{self, Day, Song};
use crate::icons::{I, Icon};

/// Go back: the history's last page, or the tab the page belongs to.
pub fn go_back(fallback: crate::Route) {
    let nav = navigator();
    if nav.can_go_back() {
        nav.go_back();
    } else {
        nav.replace(fallback);
    }
}

/// A page: its bar, then its body. A tint behind both when it is led by a cover.
#[component]
pub fn Page(#[props(default)] tint: Option<u32>, children: Element) -> Element {
    rsx! {
        section { class: "page",
            if let Some(h) = tint {
                div { class: "tintbg", style: "--h:{h}" }
            }
            {children}
        }
    }
}

/// The page's bar: a root page's title big, or the way back and the title
/// small; the tools on the right.
#[component]
pub fn Bar(
    #[props(default)] back: Option<crate::Route>,
    #[props(default)] title: Option<String>,
    #[props(default = VNode::empty())] tools: Element,
) -> Element {
    let shell = app_shell::ui::use_shell();
    let phone = shell.form() == app_shell::ui::FormFactor::Phone;
    rsx! {
        div { class: "bar",
            div { class: "lead",
                if phone && back.is_none() {
                    button {
                        class: "menubtn rb",
                        aria_label: "Menu",
                        onclick: move |_| shell.set_drawer(true),
                        Icon { name: I::PanelLeft, size: 22 }
                    }
                }
                if let Some(to) = back {
                    button {
                        class: "back rb",
                        aria_label: "Back",
                        onclick: move |_| go_back(to.clone()),
                        Icon { name: I::ChevLeft, size: 24, w: 2.2 }
                    }
                    if let Some(t) = &title {
                        span { class: "small", "{t}" }
                    }
                } else if let Some(t) = &title {
                    span { class: "title", "{t}" }
                }
            }
            div { class: "tools", {tools} }
        }
    }
}

/// A round tool button in the bar.
#[component]
pub fn Tool(
    label: String,
    icon: I,
    #[props(default)] badge: Option<String>,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            class: "tool",
            aria_label: "{label}",
            onclick: move |e| onclick.call(e),
            Icon { name: icon, size: 20 }
            if let Some(b) = badge {
                span { class: "count", "{b}" }
            }
        }
    }
}

/// Tools joined in one pill, a hairline between each.
#[component]
pub fn Joined(children: Element) -> Element {
    rsx! {
        div { class: "joined", {children} }
    }
}

#[component]
pub fn Hair() -> Element {
    rsx! {
        span { class: "hair" }
    }
}

/// Generated art: a light in the cover's colour, placed and turned by its
/// title, so no two look alike. `size` 0 fills its box.
#[component]
pub fn Cover(title: String, #[props(default)] size: u32, hue: u32) -> Element {
    let seed = title.chars().fold(5u32, |n, c| (n * 33 + c as u32) % 9973);
    let (x, y, r, s) = (
        10 + seed % 55,
        5 + (seed >> 3) % 50,
        seed % 360,
        55 + seed % 35,
    );
    let wh = if size == 0 {
        "width:0px;height:0px;".to_string()
    } else {
        format!("width:{size}px;height:{size}px;")
    };
    rsx! {
        span {
            class: "cover",
            style: "{wh}--h:{hue};--x:{x}%;--y:{y}%;--r:{r}deg;--s:{s}%",
            i {}
            u {}
        }
    }
}

/// An image, or a generated cover where there is none.
#[component]
pub fn Art(img: Option<Asset>, title: String, hue: u32, #[props(default)] size: u32) -> Element {
    rsx! {
        if let Some(src) = img {
            img { src: src, alt: "" }
        } else {
            Cover { title, size, hue }
        }
    }
}

/// A song's art: JT's cover on his own songs, a generated one otherwise.
#[component]
pub fn SongArt(song: Song) -> Element {
    let own = song.artist.is_some_and(|a| data::OWN.contains(&a));
    let hue = data::str_hue(song.artist.unwrap_or(song.title));
    rsx! {
        span { class: "sart",
            if own {
                img { src: data::JT_COVER, alt: "" }
            } else {
                Cover { title: song.title, size: 44, hue }
            }
        }
    }
}

/// A date as a calendar leaf: a small label over a big value.
#[component]
pub fn Cal(top: String, big: String) -> Element {
    rsx! {
        span { class: "cal",
            span { class: "cm", "{top}" }
            span { class: "cd", "{big}" }
        }
    }
}

/// The weekday over the day.
#[component]
pub fn DayTile(day: Day) -> Element {
    rsx! {
        Cal { top: day.dow3(), big: day.mday().to_string() }
    }
}

/// How ready a set is: a tick when full, a ring and n/N while songs go in,
/// "Not planned" when empty.
#[component]
pub fn Progress(
    n: usize,
    of: usize,
    #[props(default = "Not planned".to_string())] empty: String,
) -> Element {
    if n >= of {
        return rsx! {
            span { class: "st ok", aria_label: "Ready",
                Icon { name: I::Check, size: 13, w: 2.6 }
            }
        };
    }
    if n == 0 {
        return rsx! {
            span { class: "st none", "{empty}" }
        };
    }
    let c = 2.0 * std::f64::consts::PI * 9.0;
    let f = c * n as f64 / of as f64;
    rsx! {
        span { class: "ring", aria_label: "{n} of {of} songs",
            svg { width: "24", height: "24", view_box: "0 0 24 24",
                circle {
                    cx: "12",
                    cy: "12",
                    r: "9",
                    fill: "none",
                    stroke: "#2b2b31",
                    stroke_width: "3",
                }
                circle {
                    cx: "12",
                    cy: "12",
                    r: "9",
                    fill: "none",
                    stroke: "var(--live)",
                    stroke_width: "3",
                    stroke_linecap: "round",
                    stroke_dasharray: "{f} {c}",
                    transform: "rotate(-90 12 12)",
                }
            }
            b { "{n}/{of}" }
        }
    }
}

/// A logo ringed by how ready the next set is.
#[component]
pub fn Ring(n: usize, of: usize, size: u32) -> Element {
    let f = (n as f64 / of.max(1) as f64).min(1.0);
    let r = size as f64 / 2.0 - 2.0;
    let c = 2.0 * std::f64::consts::PI * r;
    let col = if f >= 1.0 {
        "var(--ok)"
    } else if f > 0.0 {
        "var(--live)"
    } else {
        "#34343b"
    };
    let h = size as f64 / 2.0;
    let dash = c * f;
    rsx! {
        svg {
            class: "ringsvg",
            width: "{size}",
            height: "{size}",
            view_box: "0 0 {size} {size}",
            circle {
                cx: "{h}",
                cy: "{h}",
                r: "{r}",
                fill: "none",
                stroke: "#24242a",
                stroke_width: "3",
            }
            circle {
                cx: "{h}",
                cy: "{h}",
                r: "{r}",
                fill: "none",
                stroke: "{col}",
                stroke_width: "3",
                stroke_linecap: "round",
                stroke_dasharray: "{dash} {c}",
                transform: "rotate(-90 {h} {h})",
            }
        }
    }
}

/// The set's keys in order, an outline for each place still empty.
#[component]
pub fn Keys(songs: Vec<Song>, size: usize) -> Element {
    let gaps = size.saturating_sub(songs.len());
    rsx! {
        span { class: "keys",
            for (i , x) in songs.iter().enumerate() {
                i { key: "{i}", style: "--h:{data::hue(x.key.unwrap_or(\"\")).unwrap_or(0)}", "{x.key.unwrap_or(\"\")}" }
            }
            for k in 0..gaps {
                i { key: "g{k}", class: "gap" }
            }
        }
    }
}

/// A small coloured key, as in the library and the player.
#[component]
pub fn KeyC(k: &'static str) -> Element {
    let h = data::hue(k).unwrap_or(0);
    rsx! {
        span { class: "kc", style: "--h:{h}", "{k}" }
    }
}

/// Energy, 1 to 5, as five rising bars.
#[component]
pub fn Energy(e: u8) -> Element {
    rsx! {
        span { class: "energy", aria_label: "Energy {e} of 5",
            for k in 1..=5u8 {
                i {
                    key: "{k}",
                    class: if k <= e { "on" },
                    style: "height:{4 + k * 2}px",
                }
            }
        }
    }
}

/// A track's or a record's status as a chip; a button when it can be changed.
#[component]
pub fn StageTag(
    stage: usize,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
) -> Element {
    let (name, h) = data::STAGES[stage.min(data::STAGES.len() - 1)];
    let none = h.is_none();
    let h = h.unwrap_or(0);
    rsx! {
        if let Some(cb) = onclick {
            button {
                class: "stgtag",
                style: "--h:{h}",
                "data-none": "{none}",
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    cb.call(e)
                },
                "{name}"
            }
        } else {
            span { class: "stgtag", style: "--h:{h}", "data-none": "{none}", "{name}" }
        }
    }
}

/// A person's initial in their colour.
#[component]
pub fn Avatar(who: String) -> Element {
    let h = data::person_hue(&who);
    let first = who.chars().next().unwrap_or('?');
    rsx! {
        span { class: "av", style: "--h:{h}", "{first}" }
    }
}

/// A page led by its cover: the art, then the facts, the title and its line.
#[component]
pub fn Hero(
    img: Option<Asset>,
    hue: u32,
    title: String,
    #[props(default)] kicker: Option<String>,
    facts: Element,
    sub: Element,
) -> Element {
    rsx! {
        div { class: "hero2",
            div { class: "art",
                Art { img, title: title.clone(), hue }
            }
            div { class: "htxt",
                div { class: "hfacts", {facts} }
                div { class: "hbottom",
                    if let Some(k) = kicker {
                        div { class: "hk", "{k}" }
                    }
                    div { class: "ht", "{title}" }
                    div { class: "hs", {sub} }
                }
            }
        }
    }
}
