//! The island as a WASI component: its views read the page's props from
//! their embed input and ask through the host.

use architect_extension_wasi::{Extension, embed, host};
use dioxus::prelude::*;
use serde_json::Value;

use crate::views::{self, playhead_of};

fn props() -> Value {
    embed().map_or(Value::Null, |e| e.props)
}

fn ask() -> views::Ask {
    Callback::new(|(action, args): (String, Value)| {
        let _ = host::run(&action, &args);
    })
}

#[component]
fn Progress() -> Element {
    rsx! { views::Progress { p: playhead_of(&props()), ask: ask() } }
}

#[component]
fn Transport() -> Element {
    rsx! { views::Transport { p: playhead_of(&props()), ask: ask() } }
}

fn build() -> Extension {
    Extension::new()
        .view("progress", Progress)
        .view("transport", Transport)
}

architect_extension_wasi::export!(build);
