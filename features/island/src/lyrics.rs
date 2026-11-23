//! The Lyrics tab's view (web bundle only): Session's lyrics panel
//! (`lyrics_ui`, the same one Session's window and the setlists song
//! workspace draw), fixed to its Confidence Monitor, over the song the
//! page is playing.
//!
//! Given, besides where the song is: on a share page `lyrics` (`[{at,
//! text}]`, the page's timed lyrics list for the song, seconds on its
//! recording), `sections` (the progress bar's), `title` and `artist`; in
//! the app, the playhead's state of this extension's source, whose item's
//! sung `marks` are its timed lines (`queue::lyric_lines`). It asks
//! nothing (the Confidence Monitor has no place to tap). A song with no
//! timed lines draws nothing on a share page (the page keeps its own list
//! then); in the app it names the song playing, so the tab follows the
//! playhead rather than stay blank (`queue::lyrics_song`).

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
    let stamps = crate::queue::lyric_lines(props);
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
        ["lyrics", "sections", "title", "artist", "source", "item"]
            .map(|k| p.get(k).cloned().unwrap_or(Value::Null))
    });
    let words = use_memo(move || {
        let [lyrics, sections, title, artist, source, item] = song();
        let mut props = serde_json::json!({
            "sections": sections, "title": title, "artist": artist, "source": source, "item": item,
        });
        // Only a page that gives lines: its absence lets the item's marks in.
        if let (false, Some(map)) = (lyrics.is_null(), props.as_object_mut()) {
            map.insert("lyrics".into(), lyrics);
        }
        words_of(&props)
    });
    let named = use_memo(move || {
        let [_, _, title, artist, source, _] = song();
        crate::queue::lyrics_song(
            &serde_json::json!({ "title": title, "artist": artist, "source": source }),
        )
    });
    let at = page
        .read()
        .get("time")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let (w, h) = vp();
    if words.read().is_empty() {
        return match named() {
            Some((title, artist)) => rsx! { Unsynced { title, artist } },
            None => rsx! {},
        };
    }
    if w <= 0.0 || h <= 0.0 {
        return rsx! {};
    }
    rsx! {
        LyricsView { key: "{w}x{h}", words: words(), at, only: View::Confidence, start_size: (w, h) }
    }
}

/// The song playing in the app, named, when it has no timed lines: its
/// title, who it is by, and that its words are not timed yet. Centred in
/// the frame, in the app's theme (its variables, system colours before).
#[component]
fn Unsynced(title: String, artist: String) -> Element {
    rsx! {
        div {
            "data-testid": "lyrics-unsynced",
            style: "box-sizing: border-box; min-height: 100vh; display: flex; flex-direction: column; \
                    align-items: center; justify-content: center; gap: 6px; padding: 24px; text-align: center; \
                    font-family: var(--font, system-ui, -apple-system, sans-serif); color: var(--foreground, CanvasText);",
            p { style: "margin: 0; font-size: 22px; font-weight: 700; line-height: 1.25;", "{title}" }
            if !artist.is_empty() {
                p { style: "margin: 0; font-size: 15px; color: var(--muted-foreground, GrayText);", "{artist}" }
            }
            p { style: "margin: 14px 0 0; font-size: 13px; color: var(--muted-foreground, GrayText);",
                "No timed lyrics for this song yet."
            }
        }
    }
}
