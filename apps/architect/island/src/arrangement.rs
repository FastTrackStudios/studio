//! The song playing as an arrangement, read only: its sections and its
//! sung lines on one timeline, the playhead moving through them, a tap
//! on a section or a line playing from there
//! (`docs/design/share-link-setlist.md`, the arrangement for a listener).
//!
//! Drawn from what the app's playhead says of this extension's source:
//! the item's `sections` (the chart's, on the chart's clock, where the
//! song view set the source), its `marks` (the timed lyric lines), its
//! length and where it is. Nothing of the session reaches a browser yet:
//! track lanes, markers and the reference's waveform come with the
//! arrangement Studio writes beside the song (the plan's A1), and are
//! drawn here when they do.

use dioxus::prelude::*;
use serde_json::{Value, json};

use crate::queue;
use crate::views::{Ask, playhead_of};

/// Points a second: a four-minute song is a little over three phone widths.
const PX: f64 = 5.0;

const CSS: &str = r"
.av { font-family: var(--font, system-ui, sans-serif); color: var(--foreground, CanvasText); padding: 4px 12px 16px; }
.av-head { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; padding: 4px 2px 10px; }
.av-where { font-size: 17px; font-weight: 650; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.av-song { color: var(--muted-foreground, GrayText); font-weight: 400; }
.av-time { font-size: 13px; color: var(--muted-foreground, GrayText); font-variant-numeric: tabular-nums; white-space: nowrap; }
.av-scroll { overflow-x: auto; overflow-y: hidden; border: 1px solid var(--border, rgba(128,128,128,.3)); border-radius: 12px;
  scrollbar-width: thin; }
.av-lanes { position: relative; }
.av-ruler { position: relative; height: 26px; border-bottom: 1px solid var(--border, rgba(128,128,128,.3)); }
.av-tick { position: absolute; top: 0; bottom: 0; border-left: 1px solid var(--border, rgba(128,128,128,.3));
  padding: 6px 0 0 4px; font-size: 12px; color: var(--muted-foreground, GrayText); font-variant-numeric: tabular-nums; }
.av-secs { position: relative; height: 44px; border-bottom: 1px solid var(--border, rgba(128,128,128,.3)); }
.av-sec { position: absolute; top: 0; bottom: 0; border: 0; padding: 0 6px; margin: 0; font: inherit; font-size: 13px; font-weight: 600;
  color: #fff; text-align: left; white-space: nowrap; overflow: hidden; cursor: pointer; box-shadow: inset -1px 0 0 rgba(255,255,255,.55); }
.av-sec[data-current] { box-shadow: inset 0 -4px 0 rgba(255,255,255,.75), inset -1px 0 0 rgba(255,255,255,.55); }
.av-sec:focus-visible { outline: 2px solid var(--foreground, CanvasText); outline-offset: -2px; }
.av-lines { position: relative; height: 56px; }
.av-line { position: absolute; top: 10px; bottom: 10px; width: 12px; margin-left: -6px; border: 0; padding: 0; background: transparent; cursor: pointer; }
.av-line::before { content: ''; position: absolute; top: 0; bottom: 0; left: 5px; width: 2px; border-radius: 1px;
  background: var(--muted-foreground, GrayText); opacity: .55; }
.av-line[data-past]::before { background: var(--accent, Highlight); opacity: 1; }
.av-head2 { position: absolute; left: 6px; top: 2px; font-size: 12px; color: var(--muted-foreground, GrayText); pointer-events: none; }
.av-play { position: absolute; top: 0; bottom: 0; width: 2px; margin-left: -1px; background: var(--accent, Highlight); pointer-events: none; }
.av-note { margin: 12px 2px 0; font-size: 13px; line-height: 1.45; color: var(--muted-foreground, GrayText); }
.av-empty { margin: 24px 2px; font-size: 15px; color: var(--muted-foreground, GrayText); }
";

/// Scrolls the lanes `id` sideways so the playhead (150 points past
/// `left`) is on show, when it is not.
fn keep_in_view(id: &str, left: f64) {
    use js_sys::Reflect;
    use wasm_bindgen::JsCast as _;
    let get = |o: &wasm_bindgen::JsValue, k: &str| Reflect::get(o, &k.into()).ok();
    let Some(doc) = get(&js_sys::global(), "document") else {
        return;
    };
    let Some(el) = get(&doc, "getElementById")
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .and_then(|f| f.call1(&doc, &id.into()).ok())
        .filter(wasm_bindgen::JsValue::is_object)
    else {
        return;
    };
    let number = |k: &str| get(&el, k).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let (shown, wide, head) = (number("scrollLeft"), number("clientWidth"), left + 150.0);
    if head < shown + 40.0 || head > shown + wide - 60.0 {
        let _ = Reflect::set(&el, &"scrollLeft".into(), &left.into());
    }
}

fn clock(t: f64) -> String {
    queue::clock(t.max(0.0))
}

/// The arrangement of the song playing on this extension's source.
#[component]
pub fn Arrangement(page: Signal<Value>, ask: Ask) -> Element {
    let props = page.read().clone();
    let p = playhead_of(&props);
    let title = props
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let lines = queue::lyric_lines(&props);
    if queue::now_of(&props).is_none() && p.duration <= 0.0 {
        return rsx! {
            document::Style { {CSS} }
            div { class: "av", "data-testid": "studio-arrangement",
                p { class: "av-empty", "Play a song to see its arrangement." }
            }
        };
    }
    let length = p.duration.max(1.0);
    let width = (length * PX).ceil();
    let sections: Vec<(String, f64, f64, String)> = if p.sections.is_empty() {
        vec![(
            "The whole song".to_owned(),
            0.0,
            length,
            "#64748b".to_owned(),
        )]
    } else {
        p.sections.clone()
    };
    let current = sections.iter().rposition(|s| s.1 <= p.time + 0.001);
    let here = current
        .and_then(|i| sections.get(i))
        .map(|s| s.0.clone())
        .unwrap_or_default();
    let ticks: Vec<f64> = (0..)
        .map(|i| f64::from(i) * 15.0)
        .take_while(|t| *t < length)
        .collect();
    let at = (p.time.min(length) * PX).round();
    let seek = move |t: f64| ask.call(("PLAYHEAD_SEEK".to_owned(), json!({ "t": t })));
    // The playhead kept in view: the lanes scroll under it once it has
    // passed the middle (the browser eases a short move itself).
    let scroll_to = (at - 150.0).max(0.0);
    use_effect(use_reactive!(|scroll_to| keep_in_view(
        "av-scroll",
        scroll_to
    )));
    rsx! {
        document::Style { {CSS} }
        div { class: "av", "data-testid": "studio-arrangement", "data-time": "{p.time:.2}",
            div { class: "av-head",
                span { class: "av-where",
                    if here.is_empty() { "{title}" } else { "{here} " span { class: "av-song", "{title}" } }
                }
                span { class: "av-time", "{clock(p.time)} of {clock(length)}" }
            }
            div { class: "av-scroll", id: "av-scroll", tabindex: "0", "aria-label": "The arrangement of {title}; scroll sideways",
                div { class: "av-lanes", style: "width: {width}px;",
                    div { class: "av-ruler", "aria-hidden": "true",
                        for t in ticks {
                            span { key: "t{t}", class: "av-tick", style: "left: {t * PX}px;", "{clock(t)}" }
                        }
                    }
                    div { class: "av-secs", role: "group", "aria-label": "Sections",
                        for (i, (name, start, end, color)) in sections.iter().cloned().enumerate() {
                            button { key: "s{i}", r#type: "button", class: "av-sec",
                                "data-testid": "arrangement-section",
                                "data-current": if current == Some(i) { "" },
                                "aria-current": if current == Some(i) { "true" },
                                "aria-label": "{name}, at {clock(start)}, play from here",
                                style: "left: {start * PX}px; width: {((end - start) * PX).max(2.0)}px; background: {color};",
                                onclick: move |_| seek(start),
                                "{name}"
                            }
                        }
                    }
                    if !lines.is_empty() {
                        div { class: "av-lines", role: "group", "aria-label": "Sung lines",
                            span { class: "av-head2", "aria-hidden": "true", "Lyrics, {lines.len()} lines" }
                            for (i, (t, text)) in lines.iter().cloned().enumerate() {
                                button { key: "l{i}", r#type: "button", class: "av-line",
                                    "data-past": if t <= p.time { "" },
                                    "aria-label": "{text}, at {clock(t)}, play from here",
                                    style: "left: {t * PX}px;",
                                    onclick: move |_| seek(t),
                                }
                            }
                        }
                    }
                    span { class: "av-play", "data-testid": "arrangement-playhead", style: "left: {at}px;", "aria-hidden": "true" }
                }
            }
            p { class: "av-note",
                "The song's sections and sung lines on its timeline. Tap one to play from there. Nothing here can be changed."
            }
        }
    }
}

/// A view of the More menu that has nothing to draw from this link yet:
/// what it is for, and why it is empty here.
#[component]
pub fn NotHere(title: String, what: String, why: String) -> Element {
    rsx! {
        document::Style { {CSS} }
        div { class: "av", "data-testid": "studio-not-here",
            div { class: "av-head", span { class: "av-where", "{title}" } }
            p { class: "av-empty", style: "margin: 8px 2px 6px; color: var(--foreground, CanvasText);", "{what}" }
            p { class: "av-note", "{why}" }
        }
    }
}
