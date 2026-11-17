//! The Lighting tab's view, drawn (web bundle): the song's light-show
//! video, muted, kept on the playhead at every update of the props
//! ([`crate::lighting::correction`]); under it a Review button that hands
//! the song's note and the video to `architect.review`.
//!
//! The contract — which video, from where, how it follows — is
//! [`crate::lighting`]'s, tested on the host.

use dioxus::prelude::*;
use serde_json::Value;
use wasm_bindgen::JsCast as _;
use wasm_bindgen::prelude::*;

use crate::lighting::{self, ReviewTarget, Video};

const ELEMENT: &str = "studio-lights-video";

fn get(v: &JsValue, key: &str) -> JsValue {
    js_sys::Reflect::get(v, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED)
}

/// The view's `<video>` element, once drawn.
fn element() -> Option<JsValue> {
    let document = get(&js_sys::global(), "document");
    let by_id = get(&document, "getElementById").dyn_into::<js_sys::Function>().ok()?;
    by_id
        .call1(&document, &JsValue::from_str(ELEMENT))
        .ok()
        .filter(JsValue::is_object)
}

fn call(target: &JsValue, name: &str) {
    if let Ok(f) = get(target, name).dyn_into::<js_sys::Function>() {
        // `play()` returns a promise refused while the frame has not been
        // touched; the next update asks again.
        let _ = f.call0(target);
    }
}

/// Puts the element on the playhead.
fn follow(time: f64, playing: bool) {
    let Some(video) = element() else { return };
    let now = get(&video, "currentTime").as_f64().unwrap_or(0.0);
    let paused = get(&video, "paused").as_bool().unwrap_or(true);
    let fix = lighting::correction(now, time, playing);
    if let Some(t) = fix.seek {
        let _ = js_sys::Reflect::set(&video, &"currentTime".into(), &t.into());
    }
    match (fix.play, paused) {
        (true, true) => call(&video, "play"),
        (false, false) => call(&video, "pause"),
        _ => {}
    }
}

#[component]
pub fn Lighting(page: Signal<Value>, note: Signal<Value>, ask: crate::views::Ask) -> Element {
    let video = use_memo(move || lighting::video_of(&page.read(), &note.read()));
    use_effect(move || {
        let (time, playing) = lighting::playhead(&page.read());
        follow(time, playing);
    });
    let review = video
        .read()
        .as_ref()
        .and_then(|v| lighting::review_target(v, &page.read()));
    let shell = "box-sizing: border-box; min-height: 100vh; display: flex; flex-direction: column; \
                 background: #000; color: var(--foreground, #e5e7eb); \
                 font-family: var(--font, system-ui, -apple-system, sans-serif);";
    match video() {
        None => rsx! {
            div { style: "{shell} align-items: center; justify-content: center; padding: 24px; text-align: center;",
                "data-testid": "lights-idle",
                p { style: "margin: 0; font-size: 15px; color: var(--muted-foreground, #9ca3af);",
                    "Play a song of the setlist to see its light show."
                }
            }
        },
        Some(Video { path, url: None }) => rsx! {
            div { style: "{shell} align-items: center; justify-content: center; padding: 24px; text-align: center; gap: 8px;",
                "data-testid": "lights-unavailable",
                p { style: "margin: 0; font-size: 17px; font-weight: 600;", "Light show" }
                p { style: "margin: 0; font-size: 13px; color: var(--muted-foreground, #9ca3af); word-break: break-word;",
                    "{path}"
                }
                p { style: "margin: 0; font-size: 13px; color: var(--muted-foreground, #9ca3af);",
                    "This app cannot play a song's video in a view yet."
                }
                Review { target: review, ask }
            }
        },
        Some(Video { url: Some(url), .. }) => rsx! {
            div { style: "{shell}",
                video {
                    id: ELEMENT,
                    "data-testid": "lights-video",
                    src: "{url}",
                    muted: true,
                    playsinline: true,
                    preload: "auto",
                    style: "width: 100%; flex: 1 1 auto; min-height: 0; object-fit: contain; background: #000;",
                }
                Review { target: review, ask }
            }
        },
    }
}

/// Review the light show: hands the song's note and its video to
/// Architect's Review (`services.call("architect.review", "open", {path,
/// file})`, [`lighting::review_ask`]), which draws the comments.
#[component]
fn Review(target: Option<ReviewTarget>, ask: crate::views::Ask) -> Element {
    let Some(target) = target else {
        return rsx! {};
    };
    rsx! {
        button {
            "data-testid": "lights-review",
            style: "flex: 0 0 auto; margin: 10px auto; padding: 8px 16px; border-radius: 999px; \
                    border: 1px solid var(--border, #374151); background: transparent; \
                    color: inherit; font: inherit; font-size: 14px; cursor: pointer;",
            onclick: move |_| ask.call(lighting::review_ask(&target)),
            "Review"
        }
    }
}
