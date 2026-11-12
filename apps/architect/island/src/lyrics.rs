//! The Lyrics tab's view (web bundle only): Session's lyrics panel
//! (`lyrics_ui`, the same one Session's window and the setlists song
//! workspace draw), fixed to its Confidence Monitor, over the song the
//! page is playing.
//!
//! Given, besides where the song is: `lyrics` (`[{at, text}]`, the page's
//! timed lyrics list for the song, seconds on its recording), `sections`
//! (the progress bar's), `title` and `artist`. It asks nothing (the
//! Confidence Monitor has no place to tap). A song with no timed lines
//! draws nothing: the page keeps its own list then.

use dioxus::prelude::*;
use lyrics_ui::view::{LyricsView, View, Words};
use serde_json::Value;
use wasm_bindgen::JsCast as _;
use wasm_bindgen::prelude::*;

fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_owned()
}

fn seconds(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(f64::NAN)
}

/// The song's words from the page's props.
pub fn words_of(props: &Value) -> Words {
    let list = |key: &str| {
        props
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let stamps = list("lyrics")
        .iter()
        .map(|l| (seconds(l, "at"), text(l, "text")))
        .collect();
    let sections = list("sections")
        .iter()
        .map(|s| {
            (
                text(s, "name"),
                seconds(s, "start"),
                seconds(s, "end"),
                s.get("color").and_then(Value::as_str).map(str::to_owned),
            )
        })
        .collect();
    let artist = Some(text(props, "artist")).filter(|a| !a.is_empty());
    Words::from_page(stamps, sections, text(props, "title"), artist)
}

/// The frame's size (its window's inner width and height).
fn viewport() -> (f64, f64) {
    let read = |name: &str| {
        js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str(name))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
    };
    (read("innerWidth"), read("innerHeight"))
}

/// The frame's size, kept: the page sizes this frame (it fills the
/// lyrics' place, whatever height it says), and its window says when it
/// changed (shown, a phone turned, the bars around it moving).
fn use_viewport() -> Signal<(f64, f64)> {
    let mut size = use_signal(viewport);
    use_hook(move || {
        let on = Closure::<dyn FnMut()>::new(move || {
            let now = viewport();
            if *size.peek() != now {
                size.set(now);
            }
        });
        if let Some(add) =
            js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("addEventListener"))
                .ok()
                .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        {
            let _ = add.call2(&js_sys::global(), &JsValue::from_str("resize"), on.as_ref());
        }
        // Lives as long as the frame.
        on.forget();
    });
    size
}

/// The panel, fixed to the Confidence Monitor, filling the frame.
///
/// The panel measures itself when mounted and then on a tick; made again
/// at each new size of the frame, its words are set to the new box at
/// once rather than up to a tick late.
#[component]
pub fn Lyrics(page: Signal<Value>) -> Element {
    let vp = use_viewport();
    // The song (its lines, sections, title) apart from where it is: the
    // words are worked out again only when the song changes, not at every
    // tick of the playhead.
    let song = use_memo(move || {
        let p = page.read();
        ["lyrics", "sections", "title", "artist"].map(|k| p.get(k).cloned().unwrap_or(Value::Null))
    });
    let words = use_memo(move || {
        let [lyrics, sections, title, artist] = song();
        words_of(&serde_json::json!({
            "lyrics": lyrics, "sections": sections, "title": title, "artist": artist,
        }))
    });
    let at = page
        .read()
        .get("time")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let (w, h) = vp();
    if words.read().is_empty() || w <= 0.0 || h <= 0.0 {
        return rsx! {};
    }
    rsx! {
        LyricsView { key: "{w}x{h}", words: words(), at, only: View::Confidence, start_size: (w, h) }
    }
}
