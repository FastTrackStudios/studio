//! Live: a set's chart and lyrics, a song at a time. The song on show is
//! the one playing: one line names it, the progress bars sit under it, the
//! page fills the rest, and the transport at the foot moves through it.

use dioxus::prelude::*;

use crate::Route;
use crate::data::{self, BARS, SECS};
use crate::icons::{I, Icon};
use crate::state::use_app;

#[component]
pub fn Chart(id: String) -> Element {
    rsx! {
        SheetView { id, what: "Chart", says: "The chart page (keyflow) fills this page, as wide as the phone" }
    }
}

#[component]
pub fn Lyrics(id: String) -> Element {
    rsx! {
        SheetView { id, what: "Lyrics", says: "The lyrics page fills this page, as wide as the phone" }
    }
}

#[component]
fn SheetView(id: String, what: &'static str, says: &'static str) -> Element {
    let app = use_app();
    let following = app.sync.following();
    rsx! {
        section { class: "page",
            div { class: "body sheetview",
                if let Some(name) = following {
                    div {
                        style: "align-self:center;margin:4px auto 0;padding:3px 10px;border-radius:10px;background:#1d4ed8;color:#fff;font:600 12px/1.4 system-ui,sans-serif;",
                        role: "status",
                        "Following {name}"
                    }
                }
                SongHead { id }
                div { class: "sheetwrap",
                    div { class: "sheetpage",
                        b { "{what}" }
                        span { "{says}" }
                    }
                }
            }
        }
    }
}

/// The song on show, its key, and the progress bars.
#[component]
fn SongHead(id: String) -> Element {
    let app = use_app();
    let l = (app.live)();
    let songs = (app.live_list)();
    let Some(x) = songs.get(l.song).or(songs.first()).cloned() else {
        return rsx! {};
    };
    let k = x.key.clone();
    let d = match (&x.own_key, &k) {
        (Some(o), Some(k)) if o != k => data::steps(o, k),
        _ => 0,
    };
    let sign = if d > 0 { "+" } else { "−" };
    let set = (app.live_set)();
    let back = if set.id == id && set.owner != (app.here)() {
        Route::OrgSet {
            owner: set.owner.clone(),
            id: id.clone(),
        }
    } else {
        Route::set(&id)
    };
    let n = songs.len();
    let cur = SECS[l.sec];
    let nbars = BARS[l.sec];
    rsx! {
        div { class: "nowline",
            button {
                class: "back rb sm",
                aria_label: "Back",
                onclick: move |_| {
                    navigator().replace(back.clone());
                },
                Icon { name: I::ChevLeft, size: 22, w: 2.2 }
            }
            span { class: "nn", "{l.song + 1}/{n}" }
            span { class: "t1", "{x.title}" }
            span { class: "nk",
                b { "{k.clone().unwrap_or_default()}" }
                if d != 0 {
                    " "
                    small { "{sign}{d.abs()} from {x.own_key.clone().unwrap_or_default()}" }
                }
                if let Some(t) = x.tempo {
                    " · {t} bpm"
                }
            }
        }
        div { class: "prog", aria_label: "Song progress",
            for (k , n) in SECS.iter().enumerate() {
                {
                    let fill = if k < l.sec {
                        100.0
                    } else if k == l.sec {
                        (l.bar as f64 + 0.5) / BARS[k] as f64 * 100.0
                    } else {
                        0.0
                    };
                    rsx! {
                        span {
                            key: "{k}",
                            class: if k == l.sec { "pseg now" } else { "pseg" },
                            style: "flex:{BARS[k]};--h:{data::sec_hue(n)}",
                            i { style: "width:{fill}%" }
                            b { "{data::sec_short(n)}" }
                        }
                    }
                }
            }
        }
        div {
            class: "secbar",
            aria_label: "{cur}, bar {l.bar + 1} of {nbars}",
            style: "--h:{data::sec_hue(cur)}",
            span { class: "sname", "{cur}" }
            span { class: "pbars",
                for k in 0..nbars {
                    span {
                        key: "{k}",
                        class: if k < l.bar { "pbar played" } else if k == l.bar { "pbar now" } else { "pbar" },
                    }
                }
            }
            span { class: "scount", "{l.bar + 1}/{nbars}" }
        }
    }
}
