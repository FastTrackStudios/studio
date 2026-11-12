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

/// Draw view `view` (`progress` or `transport`) into the frame's `#main`.
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

#[component]
fn App() -> Element {
    let mut props = use_signal(|| Value::Null);
    // The page's props, as they come: through a channel into the view's
    // own task, so the signal is written where Dioxus runs.
    use_hook(move || {
        let (tx, mut rx) = futures_channel::mpsc::unbounded::<String>();
        if let Some(page) = page() {
            if let Some(subscribe) = method(&page, "subscribe") {
                let on = Closure::<dyn FnMut(JsValue)>::new(move |v: JsValue| {
                    if let Ok(text) = js_sys::JSON::stringify(&v) {
                        let _ = tx.unbounded_send(String::from(text));
                    }
                });
                let _ = subscribe.call1(&page, on.as_ref());
                // Lives as long as the frame.
                on.forget();
            }
        }
        spawn(async move {
            while let Some(text) = rx.next().await {
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    props.set(v);
                }
            }
        });
    });
    let p = playhead_of(&props.read());
    let view = VIEW.with(|v| v.borrow().clone());
    match view.as_str() {
        "progress" => rsx! { views::Progress { p, ask: ask() } },
        "transport" => rsx! { views::Transport { p, ask: ask() } },
        _ => rsx! {},
    }
}
