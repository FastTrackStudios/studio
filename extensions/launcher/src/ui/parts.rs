//! The pieces every page shares: icons, covers, the date tile, how ready a
//! set is, a status chip, a key chip, a pushed page's header, a section.

use dioxus::prelude::*;

use super::data::{self, STAGES};
use super::{EDGE, INK, INK3, LIVE, OK, SURFACE, SURFACE2, hsl, image};

// ── Icons (lucide's), as shapes Blitz draws ───────────────────────────────

#[derive(Clone, Copy)]
enum Shape {
    P(&'static str),
    C(f32, f32, f32),
    R(f32, f32, f32, f32, f32),
}
use Shape::{C, P, R};

fn shapes(name: &str) -> &'static [Shape] {
    match name {
        "back" => &[P("m15 18-6-6 6-6")],
        "chev" => &[P("m9 18 6-6-6-6")],
        "plus" => &[P("M5 12h14"), P("M12 5v14")],
        "more" => &[C(12.0, 12.0, 1.0), C(19.0, 12.0, 1.0), C(5.0, 12.0, 1.0)],
        "check" => &[P("M20 6 9 17l-5-5")],
        "pin" => &[
            P(
                "M20 10c0 4.99-5.54 10.19-7.4 11.79a1 1 0 0 1-1.2 0C9.54 20.19 4 14.99 4 10a8 8 0 0 1 16 0",
            ),
            C(12.0, 10.0, 3.0),
        ],
        "clock" => &[C(12.0, 12.0, 9.0), P("M12 7v5l3 2")],
        "mic" => &[
            P("M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"),
            P("M19 10v2a7 7 0 0 1-14 0v-2"),
            P("M12 19v3"),
        ],
        "guitar" => &[
            C(8.0, 16.0, 5.0),
            C(8.0, 16.0, 1.3),
            P("M11.6 12.4 19.5 4.5"),
            P("m18 2.5 3.5 3.5"),
        ],
        "acoustic" => &[
            C(9.0, 15.0, 6.0),
            C(9.0, 15.0, 1.8),
            P("M13.3 10.7 20.5 3.5"),
            P("m19 2 3 3"),
        ],
        "bass" => &[
            C(7.5, 16.5, 4.5),
            P("M10.7 13.3 21 3"),
            P("m19.5 1.5 3 3"),
            P("M5.5 16.5h4"),
        ],
        "keys" => &[
            R(2.0, 5.0, 20.0, 14.0, 2.0),
            P("M8 13v6M14 13v6"),
            P("M6.5 5v8h3V5M12.5 5v8h3V5M18 5v8h2.5"),
        ],
        "drums" => &[
            P("M3 9c0-1.7 4-3 9-3s9 1.3 9 3-4 3-9 3-9-1.3-9-3"),
            P("M3 9v7c0 1.7 4 3 9 3s9-1.3 9-3V9"),
            P("m17 2-4 6M7 2l4 6"),
        ],
        "perc" => &[P("M7 3h10l-1.5 18h-7z"), P("M7.4 8h9.2M8 15h8")],
        "branch" => &[
            C(6.0, 6.0, 2.5),
            C(6.0, 18.0, 2.5),
            C(18.0, 8.0, 2.5),
            P("M6 8.5v7"),
            P("M18 10.5c0 4-6 3.5-10.5 6"),
        ],
        "message" => &[P("M7.9 20A9 9 0 1 0 4 16.1L2 22Z")],
        "play" => &[P("M6 3l14 9-14 9z")],
        "pause" => &[R(6.0, 4.0, 4.0, 16.0, 1.0), R(14.0, 4.0, 4.0, 16.0, 1.0)],
        "library" => &[P("m16 6 4 14"), P("M12 6v14"), P("M8 8v12"), P("M4 4v16")],
        "search" => &[C(11.0, 11.0, 8.0), P("m21 21-4.3-4.3")],
        "setlist" => &[
            P("M21 15V6"),
            C(18.5, 15.5, 2.5),
            P("M12 12H3"),
            P("M16 6H3"),
            P("M12 18H3"),
        ],
        "calendar" => &[R(3.0, 4.0, 18.0, 18.0, 2.0), P("M16 2v4M8 2v4M3 10h18")],
        "disc" => &[C(12.0, 12.0, 10.0), C(12.0, 12.0, 2.0)],
        "arrow-up" => &[P("m5 12 7-7 7 7"), P("M12 19V5")],
        _ => &[C(12.0, 12.0, 9.0)],
    }
}

#[component]
pub fn Icon(
    name: String,
    #[props(default = 20)] size: u32,
    #[props(default = 1.8)] stroke: f32,
    #[props(default)] fill: bool,
) -> Element {
    let fill = if fill { "currentColor" } else { "none" };
    rsx! {
        svg {
            width: "{size}",
            height: "{size}",
            view_box: "0 0 24 24",
            fill: "{fill}",
            stroke: "currentColor",
            stroke_width: "{stroke}",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            style: "flex-shrink: 0; display: block;",
            for (i, s) in shapes(&name).iter().enumerate() {
                match *s {
                    P(d) => rsx! { path { key: "{i}", d: "{d}" } },
                    C(cx, cy, r) => rsx! { circle { key: "{i}", cx: "{cx}", cy: "{cy}", r: "{r}" } },
                    R(x, y, w, h, rx) => rsx! { rect { key: "{i}", x: "{x}", y: "{y}", width: "{w}", height: "{h}", rx: "{rx}" } },
                }
            }
        }
    }
}

// ── Covers ────────────────────────────────────────────────────────────────

/// A cover: the image when there is one, else soft light in its colour.
/// `size` 0 fills the box it is in.
#[component]
pub fn Cover(
    art: Option<String>,
    hue: u16,
    #[props(default)] size: u32,
    #[props(default = 10)] radius: u32,
) -> Element {
    let dim = if size == 0 {
        "width: 100%; height: 100%;".to_owned()
    } else {
        format!("width: {size}px; height: {size}px;")
    };
    let edge = "box-shadow: inset 0 0 0 1px rgba(255,255,255,0.08);";
    if let Some(src) = art {
        let src = image(&src);
        return rsx! {
            img { src: "{src}", style: "{dim} border-radius: {radius}px; object-fit: cover; display: block; flex-shrink: 0; {edge}" }
        };
    }
    let a = hsl(hue, 70, 62);
    let b = hsl((hue + 300) % 360, 75, 50);
    let c = hsl(hue, 40, 9);
    let d = hsl(hue, 45, 30);
    rsx! {
        div {
            style: "{dim} flex-shrink: 0; border-radius: {radius}px; {edge} \
                    background: radial-gradient(circle at 65% 62%, {a} 0%, {b} 26%, transparent 55%), \
                    radial-gradient(circle at 25% 15%, {d} 0%, {c} 70%);",
        }
    }
}

// ── The date tile ─────────────────────────────────────────────────────────

/// A small red label over a big value: a weekday over its day in a list,
/// a month over its day in a header, a month over a year for a show.
#[component]
pub fn Cal(top: String, big: String) -> Element {
    rsx! {
        div {
            style: "width: 40px; height: 40px; flex-shrink: 0; border-radius: 9px; display: flex; flex-direction: column; \
                    align-items: center; justify-content: center; background: rgba(255,255,255,0.04); \
                    box-shadow: inset 0 0 0 1px rgba(255,255,255,0.09);",
            span { style: "font-size: 9px; font-weight: 800; letter-spacing: 0.05em; color: rgba(248,113,113,0.85); text-transform: uppercase;", "{top}" }
            span { style: "font-size: 16px; font-weight: 700; line-height: 1.05; color: {INK};", "{big}" }
        }
    }
}

#[must_use]
pub fn day_cal(day: data::Day) -> Element {
    rsx! { Cal { top: day.wd3().to_owned(), big: day.day_of_month().to_string() } }
}

// ── How ready a set is ────────────────────────────────────────────────────

/// A tick when full, a ring with n/N while songs go in, "Not planned" when empty.
#[component]
pub fn Ready(n: usize, of: usize) -> Element {
    if of > 0 && n >= of {
        return rsx! {
            span { style: "width: 24px; height: 24px; border-radius: 12px; flex-shrink: 0; display: flex; align-items: center; \
                           justify-content: center; background: rgba(34,197,94,0.14); color: {OK};",
                Icon { name: "check", size: 13, stroke: 2.6 }
            }
        };
    }
    if n == 0 {
        return rsx! { span { style: "font-size: 12px; color: {INK3}; white-space: nowrap; flex-shrink: 0;", "Not planned" } };
    }
    let c = 2.0 * std::f32::consts::PI * 9.0;
    let f = c * f32::from(u16::try_from(n).unwrap_or(0))
        / f32::from(u16::try_from(of.max(1)).unwrap_or(1));
    rsx! {
        span { style: "display: flex; align-items: center; gap: 6px; flex-shrink: 0;",
            svg { width: "24", height: "24", view_box: "0 0 24 24",
                circle { cx: "12", cy: "12", r: "9", fill: "none", stroke: "#2b2b31", stroke_width: "3" }
                circle { cx: "12", cy: "12", r: "9", fill: "none", stroke: "{LIVE}", stroke_width: "3", stroke_linecap: "round",
                    stroke_dasharray: "{f} {c}", transform: "rotate(-90 12 12)" }
            }
            b { style: "font-size: 13px; font-weight: 650; color: {LIVE};", "{n}/{of}" }
        }
    }
}

/// The ring round an event's logo, by how ready its next set is.
#[component]
pub fn LogoRing(n: usize, of: usize, size: u32) -> Element {
    let r = f32::from(u16::try_from(size).unwrap_or(66)) / 2.0 - 2.0;
    let c = 2.0 * std::f32::consts::PI * r;
    let frac = if of == 0 {
        0.0
    } else {
        (f32::from(u16::try_from(n).unwrap_or(0)) / f32::from(u16::try_from(of).unwrap_or(1)))
            .min(1.0)
    };
    let col = if frac >= 1.0 {
        OK
    } else if frac > 0.0 {
        LIVE
    } else {
        "#34343b"
    };
    let half = f32::from(u16::try_from(size).unwrap_or(66)) / 2.0;
    let dash = c * frac;
    rsx! {
        svg { width: "{size}", height: "{size}", view_box: "0 0 {size} {size}", style: "position: absolute; left: 0; top: 0;",
            circle { cx: "{half}", cy: "{half}", r: "{r}", fill: "none", stroke: "#24242a", stroke_width: "3" }
            circle { cx: "{half}", cy: "{half}", r: "{r}", fill: "none", stroke: "{col}", stroke_width: "3", stroke_linecap: "round",
                stroke_dasharray: "{dash} {c}", transform: "rotate(-90 {half} {half})" }
        }
    }
}

// ── Chips ─────────────────────────────────────────────────────────────────

/// A track's or a record's status.
#[component]
pub fn Stage(stage: usize, #[props(default)] small: bool) -> Element {
    let (name, hue) = STAGES.get(stage).copied().unwrap_or(("", None));
    let (bg, fg) = hue.map_or_else(
        || (SURFACE2.to_owned(), INK3.to_owned()),
        |h| (hsl(h, 45, 18), hsl(h, 75, 72)),
    );
    let fs = if small { 10.5 } else { 12.0 };
    rsx! {
        span { style: "font-size: {fs}px; font-weight: 700; padding: 2px 7px; border-radius: 6px; background: {bg}; color: {fg}; white-space: nowrap; flex-shrink: 0;",
            "{name}"
        }
    }
}

/// A key in its colour; a key moved from the song's own, a faint +n beside it.
#[component]
pub fn Key(value: Option<String>, #[props(default)] from: Option<String>) -> Element {
    let Some(key) = value else {
        return rsx! {
            span { style: "height: 26px; padding: 0 8px; border-radius: 7px; display: flex; align-items: center; font-size: 11px; \
                           font-weight: 600; color: {INK3}; box-shadow: inset 0 0 0 1px #3a3a42; flex-shrink: 0;",
                "Set key"
            }
        };
    };
    let h = data::key_hue(&key).unwrap_or(0);
    let moved = from
        .as_deref()
        .filter(|f| *f != key)
        .map(|f| data::steps(f, &key))
        .filter(|d| *d != 0);
    let (bg, fg) = (hsl(h, 40, 15), hsl(h, 70, 75));
    rsx! {
        span { style: "height: 26px; min-width: 38px; padding: 0 8px; border-radius: 7px; display: flex; align-items: baseline; \
                       justify-content: center; gap: 3px; padding-top: 5px; box-sizing: border-box; background: {bg}; flex-shrink: 0;",
            b { style: "font-size: 13.5px; font-weight: 700; color: {fg};", "{key}" }
            if let Some(d) = moved {
                small { style: "font-size: 9.5px; font-weight: 600; color: {INK3};",
                    if d > 0 { "+{d}" } else { "−{d.abs()}" }
                }
            }
        }
    }
}

// ── Layout ────────────────────────────────────────────────────────────────

/// A section's label, with an optional link at its right.
#[component]
pub fn Head(
    text: String,
    #[props(default)] link: Option<String>,
    #[props(default)] onlink: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        div { style: "display: flex; align-items: center; justify-content: space-between; margin: 22px 2px 8px;",
            span { style: "font-size: 13px; font-weight: 650; color: {INK3};", "{text}" }
            if let Some(link) = link {
                button {
                    style: "display: flex; align-items: center; gap: 2px; font-size: 12.5px; font-weight: 600; color: {INK3}; \
                            background: none; border: none; padding: 6px 0;",
                    onclick: move |_| if let Some(f) = onlink { f.call(()) },
                    "{link}"
                    Icon { name: "chev", size: 14 }
                }
            }
        }
    }
}

/// A tab's own title, large, at the top of its page.
#[component]
pub fn Title(text: String) -> Element {
    rsx! {
        div { style: "display: flex; align-items: center; justify-content: space-between; min-height: 52px; margin: 6px 0 4px;",
            span { style: "font-size: 34px; font-weight: 760; letter-spacing: -0.025em; color: {INK};", "{text}" }
        }
    }
}

/// A group of rows on one card.
#[component]
pub fn Card(children: Element) -> Element {
    rsx! {
        div { style: "background: {SURFACE}; border: 1px solid #1c1c21; border-radius: 16px; overflow: hidden; display: flex; flex-direction: column;",
            {children}
        }
    }
}

/// A pushed page's header: a round back button, its title beside it.
#[component]
pub fn Back(title: String, onback: EventHandler<()>) -> Element {
    rsx! {
        div { style: "display: flex; align-items: center; gap: 12px; margin: 0 0 10px;",
            button {
                style: "width: 40px; height: 40px; border-radius: 20px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; \
                        background: #1a1a1f; border: 1px solid {EDGE}; color: {INK};",
                onclick: move |_| onback.call(()),
                Icon { name: "back", size: 22, stroke: 2.2 }
            }
            span { style: "font-size: 17px; font-weight: 650; color: {INK}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{title}" }
        }
    }
}

/// A row's two lines of text.
#[component]
pub fn Lines(title: String, sub: String) -> Element {
    rsx! {
        div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px;",
            span { style: "font-size: 16px; font-weight: 620; color: {INK}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{title}" }
            span { style: "font-size: 13px; color: {INK3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{sub}" }
        }
    }
}

/// The header of a set, show, series or record: the square cover on the
/// left, what it is beside it.
#[component]
pub fn Hero(
    art: Option<String>,
    hue: u16,
    title: String,
    sub: String,
    #[props(default)] count: String,
    #[props(default)] facts: Option<Element>,
) -> Element {
    rsx! {
        div { style: "display: flex; gap: 16px; align-items: stretch; margin: 4px 0 6px;",
            div { style: "width: 140px; height: 140px; flex-shrink: 0; border-radius: 14px; overflow: hidden; background: #000;",
                Cover { art, hue, radius: 14 }
            }
            div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; justify-content: space-between; gap: 8px;",
                div { style: "display: flex; gap: 8px; align-items: center; flex-wrap: wrap;", {facts} }
                div {
                    div { style: "font-size: 30px; line-height: 1.12; font-weight: 750; letter-spacing: -0.02em; color: {INK};", "{title}" }
                    div { style: "font-size: 14px; color: {INK3}; margin-top: 2px;", "{sub}" }
                    if !count.is_empty() {
                        div { style: "font-size: 12.5px; color: #80808a; margin-top: 3px;", "{count}" }
                    }
                }
            }
        }
    }
}

/// Where and when, quiet: the room, the call time, the start.
#[component]
pub fn Where(room: String, call: String, starts: String) -> Element {
    rsx! {
        div { style: "display: flex; align-items: center; gap: 7px; height: 40px; padding: 0 12px 0 10px; border-radius: 10px; \
                       background: rgba(255,255,255,0.025); box-shadow: inset 0 0 0 1px rgba(255,255,255,0.06); color: #7a7a83;",
            Icon { name: "pin", size: 14, stroke: 2.0 }
            div { style: "display: flex; flex-direction: column; line-height: 1.15;",
                b { style: "font-size: 13px; font-weight: 600; color: #9a9aa3; white-space: nowrap;", "{room}" }
                small { style: "font-size: 11px; color: #85858e; white-space: nowrap;",
                    span { style: "color: rgba(240,194,75,0.75); font-weight: 600;", "Call {call}" }
                    " · Starts {starts}"
                }
            }
        }
    }
}

/// A hairline between rows of a card, inset to the text.
#[component]
pub fn Rule(#[props(default = 70)] left: u32) -> Element {
    rsx! { div { style: "height: 1px; background: #1d1d22; margin-left: {left}px;" } }
}

pub const ROW: &str = "display: flex; align-items: center; gap: 12px; width: 100%; padding: 10px 14px 10px 12px; \
    background: none; border: none; color: inherit; text-align: left; font: inherit;";
