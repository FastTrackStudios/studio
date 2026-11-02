//! The kind's view: the surface [`shown`] shows, as an inline `<svg>` in a
//! Dioxus component, redrawn at each change — its knobs dragged, an action
//! (`FRAME_SURFACE_PARAM_SET`, `FRAME_SURFACE_FEED`), a save to its
//! document.

use dioxus::prelude::*;
use frame::embed::{Button, Modifiers, Pointer};

use crate::shown;
use crate::svg::{Node, Svg};

/// The kind's name, as the pane shows it.
pub const TITLE: &str = "Frame surface";

/// How far a param's buttons move it.
const STEP: f64 = 0.1;

const BUTTON: &str = "padding: 4px 10px; border-radius: 6px; border: 1px solid #3f3f46; \
                      background: #27272a; color: #e4e4e7; font-size: 13px;";

/// The view: the surface, its document, and each param and feed with what
/// it reads; a param's two buttons move it a step.
#[component]
pub fn FrameSurface() -> Element {
    let mut revision = use_signal(|| 0_u64);
    use_future(move || async move {
        let mut rx = shown::changes();
        while rx.changed().await.is_ok() {
            let r = *rx.borrow_and_update();
            revision.set(r);
        }
    });
    let rev = revision();
    let svg = shown::paint();
    let state = shown::state();
    let document = state["document"]
        .as_str()
        .map_or_else(|| "built in".to_owned(), str::to_owned);
    let error = state["error"].as_str().map(str::to_owned);
    let params: Vec<(String, String, f64)> = state["params"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| {
            (
                p["addr"].as_str().unwrap_or_default().to_owned(),
                p["name"].as_str().unwrap_or_default().to_owned(),
                p["value"].as_f64().unwrap_or_default(),
            )
        })
        .collect();
    let feeds: Vec<(String, f64)> = state["feeds"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            (
                f["name"].as_str().unwrap_or_default().to_owned(),
                f["values"][0].as_f64().unwrap_or_default(),
            )
        })
        .collect();
    let step = |addr: String, by: f64| {
        move |_| {
            if let Some(v) = shown::param(&addr) {
                let _ = shown::set_param(&addr, v + by);
            }
        }
    };
    let at = |e: &PointerEvent| {
        let p = e.element_coordinates();
        (p.x, p.y)
    };
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 12px; padding: 20px; \
                      font-family: -apple-system, system-ui, sans-serif; color: #e4e4e7;",
            div { style: "display: flex; align-items: baseline; gap: 8px;",
                div { style: "font-size: 15px; font-weight: 650;", "{TITLE}" }
                div { style: "font-size: 12px; color: #a1a1aa; font-family: monospace;", "{crate::KIND}" }
            }
            div { style: "max-width: 420px; font-size: 13px; line-height: 1.5; color: #a1a1aa;",
                "A Frame document, laid out and painted by Frame in a process of its own. Drag the knob; a save to the document reloads it."
            }
            div { style: "font-size: 12px; color: #a1a1aa; font-family: monospace;",
                "{document}"
            }
            if let Some(e) = error {
                div { style: "max-width: 420px; color: #f87171; font-size: 12px;", "{e}" }
            }
            // The surface, and beside it each param and feed.
            div { style: "display: flex; flex-wrap: wrap; gap: 16px; align-items: flex-start;",
                match svg {
                    Ok(svg) => rsx! {
                        div {
                            style: "touch-action: none;",
                            onpointerdown: move |e| {
                                let (x, y) = at(&e);
                                shown::pointer(Pointer::Down { x, y, button: Button::Left, mods: Modifiers::default() });
                            },
                            onpointermove: move |e| {
                                let (x, y) = at(&e);
                                shown::pointer(Pointer::Move { x, y });
                            },
                            onpointerup: move |e| {
                                let (x, y) = at(&e);
                                shown::pointer(Pointer::Up { x, y, button: Button::Left });
                            },
                            onpointerleave: move |_| shown::pointer(Pointer::Leave),
                            SvgView { key: "{rev}", svg }
                        }
                    },
                    Err(e) => rsx! {
                        div { style: "color: #f87171; font-size: 13px;", "The surface did not load: {e}" }
                    },
                }
                div { style: "display: flex; flex-direction: column; gap: 8px; \
                              font-size: 13px; font-variant-numeric: tabular-nums;",
                    for (addr, name, value) in params {
                        div { key: "{addr}", style: "display: flex; align-items: center; gap: 6px;",
                            button { style: BUTTON, onclick: step(addr.clone(), -STEP), "−" }
                            button { style: BUTTON, onclick: step(addr.clone(), STEP), "+" }
                            {format!("{name} {:.0}%", value * 100.0)}
                        }
                    }
                    for (name, db) in feeds {
                        div { key: "{name}", {format!("{name} {db:.1} dB")} }
                    }
                }
            }
        }
    }
}

/// An [`Svg`] as elements. Blitz draws an inline `<svg>` from a snapshot
/// of it, so the caller keys it by a revision: each change is a new one.
#[component]
pub fn SvgView(svg: Svg) -> Element {
    let w = svg.width;
    let h = svg.height;
    rsx! {
        svg {
            width: "{w}",
            height: "{h}",
            view_box: "0 0 {w} {h}",
            if !svg.clips.is_empty() {
                defs {
                    for (i, c) in svg.clips.iter().enumerate() {
                        clipPath { key: "{i}", id: Svg::clip_id(i),
                            path { d: "{c.d}", transform: "{c.transform}" }
                        }
                    }
                }
            }
            for (i, n) in svg.nodes.iter().enumerate() {
                SvgNode { key: "{i}", node: n.clone() }
            }
        }
    }
}

#[component]
fn SvgNode(node: Node) -> Element {
    match node {
        Node::Path(p) => {
            let (fill, fill_opacity) = p.fill.unwrap_or_else(|| ("none".to_owned(), 1.0));
            let (stroke, stroke_opacity, stroke_width) =
                p.stroke.unwrap_or_else(|| ("none".to_owned(), 1.0, 0.0));
            rsx! {
                path {
                    d: p.d,
                    transform: p.transform,
                    fill,
                    fill_opacity: "{fill_opacity}",
                    fill_rule: p.fill_rule,
                    stroke,
                    stroke_opacity: "{stroke_opacity}",
                    stroke_width: "{stroke_width}",
                }
            }
        }
        Node::Group {
            clip,
            opacity,
            children,
        } => {
            let clip = clip.map_or_else(
                || "none".to_owned(),
                |c| format!("url(#{})", Svg::clip_id(c)),
            );
            let opacity = opacity.unwrap_or(1.0);
            rsx! {
                g { clip_path: clip, opacity: "{opacity}",
                    for (i, n) in children.into_iter().enumerate() {
                        SvgNode { key: "{i}", node: n }
                    }
                }
            }
        }
    }
}
