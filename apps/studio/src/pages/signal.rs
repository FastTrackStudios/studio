//! Signal: the guitar rig's files in Studio's home (`Signal/`, see
//! `docs/storage.md`). The rig itself plays in each session's mixer; this
//! page only says what the home holds, and how to add it when it holds
//! nothing.

use dioxus::prelude::*;

use crate::pieces::{Bar, Page};

/// What `<home>/Signal/` holds: each folder's name and how many entries
/// it has (none in a browser, which has no disk).
pub fn rig_folders() -> Vec<(String, usize)> {
    if cfg!(target_arch = "wasm32") {
        return Vec::new();
    }
    crate::files_live::disk::list("", "Signal")
        .into_iter()
        .filter(|h| h.dir && !h.name.starts_with('.'))
        .map(|h| {
            let n = crate::files_live::disk::list("", &h.path).len();
            (h.name, n)
        })
        .collect()
}

#[component]
pub fn Signal() -> Element {
    // Read off the UI thread: Drive's mount can stall.
    let folders = use_resource(|| crate::state::off_thread(rig_folders));
    let folders = move || folders().flatten();
    rsx! {
        Page {
            Bar { title: "Signal" }
            div { class: "body",
                if folders().is_none() {
                    p { class: "empty", "Looking in Studio's home…" }
                } else if folders().unwrap_or_default().is_empty() {
                    div { class: "empty", role: "status",
                        p { "No rig in Studio's home yet." }
                        p {
                            "Signal's captures, IRs and presets go in "
                            code { "Signal/" }
                            " in Studio's home; a session's rig is in its mixer."
                        }
                    }
                } else {
                    div { class: "agenda",
                        for (name , n) in folders().unwrap_or_default() {
                            div { key: "{name}", class: "ag",
                                span { class: "mid",
                                    span { class: "t1", "{name}" }
                                    span { class: "s1", "{n} items" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
