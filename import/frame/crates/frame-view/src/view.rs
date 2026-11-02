//! The kind's view: a [`KnobMeter`] as an inline `<svg>` in a Dioxus
//! component, redrawn whenever [`set`] moves it — from its own buttons or
//! from an action (`FRAME_SURFACE_SET`).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::LazyLock;

use dioxus::prelude::*;
use tokio::sync::watch;

use crate::surface::{KnobMeter, State};
use crate::svg::{Node, Svg};

/// How wide the surface is drawn, px.
pub const WIDTH: f64 = 212.0;

static STATE: LazyLock<watch::Sender<State>> =
    LazyLock::new(|| watch::Sender::new(State::default()));

/// Moves the surface in every view of it.
pub fn set(state: State) {
    STATE.send_replace(State {
        knob: state.knob.clamp(0.0, 1.0),
        meter: state.meter.clamp(0.0, 1.0),
    });
}

/// Where the surface is.
#[must_use]
pub fn state() -> State {
    *STATE.borrow()
}

/// The kind's name, as the pane shows it.
pub const TITLE: &str = "Frame surface";

/// The view: the surface, what it reads, and a knob's two buttons.
#[component]
pub fn FrameSurface() -> Element {
    let surface = use_hook(|| KnobMeter::new(WIDTH).map(|s| Rc::new(RefCell::new(s))));
    let mut now = use_signal(state);
    let mut revision = use_signal(|| 0_u64);
    use_future(move || async move {
        let mut rx = STATE.subscribe();
        while rx.changed().await.is_ok() {
            let s = *rx.borrow_and_update();
            now.set(s);
            revision += 1;
        }
    });
    let s = now();
    let svg = match &surface {
        Ok(surface) => {
            let mut surface = surface.borrow_mut();
            surface.set(s);
            Ok(surface.svg())
        }
        Err(e) => Err(e.clone()),
    };
    let db = -60.0 + s.meter * 66.0;
    let step = move |by: f64| {
        let s = state();
        set(State {
            knob: s.knob + by,
            ..s
        });
    };
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 12px; padding: 20px; \
                      font-family: -apple-system, system-ui, sans-serif; color: #e4e4e7;",
            div { style: "display: flex; align-items: baseline; gap: 8px;",
                div { style: "font-size: 15px; font-weight: 650;", "{TITLE}" }
                div { style: "font-size: 12px; color: #a1a1aa; font-family: monospace;", "{crate::KIND}" }
            }
            div { style: "max-width: 420px; font-size: 13px; line-height: 1.5; color: #a1a1aa;",
                "A knob and a meter from a Frame document, laid out and painted by Frame in a process of its own."
            }
            // The surface, and beside it what it reads and its knob's buttons.
            div { style: "display: flex; flex-wrap: wrap; gap: 16px; align-items: flex-start;",
                match svg {
                    Ok(svg) => rsx! {
                        for rev in std::iter::once(revision()) {
                            SvgView { key: "{rev}", svg: svg.clone() }
                        }
                    },
                    Err(e) => rsx! {
                        div { style: "color: #f87171; font-size: 13px;", "The surface did not load: {e}" }
                    },
                }
                div { style: "display: flex; flex-direction: column; gap: 8px;",
                    div { style: "font-size: 13px; font-variant-numeric: tabular-nums;",
                        {format!("Drive {:.0}%", s.knob * 100.0)}
                    }
                    div { style: "font-size: 13px; font-variant-numeric: tabular-nums;",
                        {format!("Level {db:.1} dB")}
                    }
                    button {
                        style: "padding: 6px 14px; border-radius: 6px; border: 1px solid #3f3f46; \
                                background: #27272a; color: #e4e4e7; font-size: 13px;",
                        onclick: move |_| step(0.1),
                        "Turn up"
                    }
                    button {
                        style: "padding: 6px 14px; border-radius: 6px; border: 1px solid #3f3f46; \
                                background: #27272a; color: #e4e4e7; font-size: 13px;",
                        onclick: move |_| step(-0.1),
                        "Turn down"
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
