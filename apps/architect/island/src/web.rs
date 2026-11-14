//! The island as a web bundle (`just island-web`): wasm-bindgen's
//! `--target web` glue, whose `mount(view)` Architect's web view frame
//! calls once the page has handed it the bundle. The frame's
//! `architectPage` gives where the song is (`subscribe`) and takes the
//! view's asks (`ask`); nothing else of the page reaches it.

use std::cell::RefCell;

use dioxus::prelude::*;
use futures_util::StreamExt as _;
use serde_json::Value;
use wasm_bindgen::JsCast as _;
use wasm_bindgen::prelude::*;

use crate::views::{self, playhead_of};

thread_local! {
    /// The view this frame draws.
    static VIEW: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Draw view `view` (`progress`, `transport`, `lyrics`,
/// `setlist.playlist` or `song`) into the frame's `#main`.
#[wasm_bindgen]
pub fn mount(view: String) {
    VIEW.with(|v| *v.borrow_mut() = view);
    dioxus::launch(App);
}

/// The frame's `architectPage`.
fn page() -> Option<JsValue> {
    js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("architectPage"))
        .ok()
        .filter(JsValue::is_object)
}

fn method(page: &JsValue, name: &str) -> Option<js_sys::Function> {
    js_sys::Reflect::get(page, &JsValue::from_str(name))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
}

fn ask() -> views::Ask {
    Callback::new(|(action, args): (String, Value)| {
        let Some(page) = page() else { return };
        let Some(ask) = method(&page, "ask") else {
            return;
        };
        let args = js_sys::JSON::parse(&args.to_string()).unwrap_or(JsValue::NULL);
        let _ = ask.call2(&page, &JsValue::from_str(&action), &args);
    })
}

/// One update of the page's props, as the view's task takes it: the
/// props without the note, and the note when it changed.
struct Update {
    props: String,
    note: Option<String>,
}

/// A JS value's own property, or undefined.
fn get(v: &JsValue, key: &str) -> JsValue {
    js_sys::Reflect::get(v, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED)
}

/// What says whether the handed note changed, cheap to make on every
/// update: the note with each handed image's `data:` URL (up to 8 MiB of
/// covers in all) replaced by its length; its body, its properties and
/// an `.lrc`'s text as they are. None without a note.
fn note_digest(note: &JsValue) -> Option<String> {
    if !note.is_object() {
        return None;
    }
    let copy = js_sys::Object::assign(&js_sys::Object::new(), note.unchecked_ref());
    if let Ok(files) = get(note, "files").dyn_into::<js_sys::Array>() {
        let light = js_sys::Array::new();
        for f in files.iter() {
            let one = js_sys::Object::assign(&js_sys::Object::new(), f.unchecked_ref());
            let data = get(&f, "data");
            if let Some(d) = data.dyn_ref::<js_sys::JsString>() {
                let _ = js_sys::Reflect::set(&one, &"data".into(), &d.length().into());
            }
            light.push(&one);
        }
        let _ = js_sys::Reflect::set(&copy, &"files".into(), &light);
    }
    js_sys::JSON::stringify(&copy).ok().map(String::from)
}

#[component]
fn App() -> Element {
    let mut props = use_signal(|| Value::Null);
    // The note the app hands with every update (`note`, its files' images
    // as `data:` URLs among it): kept apart, and read and written only
    // when its digest changes, so a tick of the playhead neither copies
    // the covers again nor reads the song again.
    let mut note = use_signal(|| Value::Null);
    // The page's props, as they come: through a channel into the view's
    // own task, so the signal is written where Dioxus runs.
    use_hook(move || {
        let (tx, mut rx) = futures_channel::mpsc::unbounded::<Update>();
        if let Some(page) = page() {
            if let Some(subscribe) = method(&page, "subscribe") {
                let mut last: Option<String> = None;
                let on = Closure::<dyn FnMut(JsValue)>::new(move |v: JsValue| {
                    if !v.is_object() {
                        return;
                    }
                    let n = get(&v, "note");
                    let rest = js_sys::Object::assign(&js_sys::Object::new(), v.unchecked_ref());
                    let _ = js_sys::Reflect::delete_property(&rest, &"note".into());
                    let Ok(props) = js_sys::JSON::stringify(&rest) else {
                        return;
                    };
                    let digest = note_digest(&n);
                    let note = if digest.is_some() && digest != last {
                        last = digest;
                        js_sys::JSON::stringify(&n).ok().map(String::from)
                    } else {
                        None
                    };
                    let _ = tx.unbounded_send(Update {
                        props: String::from(props),
                        note,
                    });
                });
                let _ = subscribe.call1(&page, on.as_ref());
                // Lives as long as the frame.
                on.forget();
            }
        }
        spawn(async move {
            while let Some(u) = rx.next().await {
                if let Some(n) = u.note.and_then(|n| serde_json::from_str::<Value>(&n).ok()) {
                    note.set(n);
                }
                if let Ok(v) = serde_json::from_str::<Value>(&u.props) {
                    props.set(v);
                }
            }
        });
    });
    let view = VIEW.with(|v| v.borrow().clone());
    if view == "lyrics" {
        return rsx! { crate::lyrics::Lyrics { page: props } };
    }
    if view == crate::queue::VIEW {
        return rsx! { crate::setlist::SetlistView { page: props, note, ask: ask() } };
    }
    if view == crate::song::VIEW {
        return rsx! { crate::chart::SongView { page: props, note, ask: ask() } };
    }
    let p = playhead_of(&props.read());
    match view.as_str() {
        "progress" => rsx! { views::Progress { p, ask: ask() } },
        "transport" => rsx! { views::Transport { p, ask: ask() } },
        _ => rsx! {},
    }
}
