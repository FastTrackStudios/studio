//! Signal: the live rig opened beside the session (the Rig window,
//! `STUDIO_RIG_WINDOW`: the guitar's, the bass's or the keys'), and the
//! rig's files in Studio's home (`Signal/`, see `docs/storage.md`): what
//! the home holds, and how to add it when it holds nothing. A session's
//! own Signal blocks play in its mixer.

use dioxus::prelude::*;

use crate::pieces::{Bar, Page};
use crate::state::use_app;

/// The rigs the Rig window shows, as the page offers them.
const RIGS: [&str; 3] = ["Guitar", "Bass", "Keys"];

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
    let folders = use_resource(|| crate::state::read_home(rig_folders));
    let app = use_app();
    rsx! {
        Page {
            Bar { title: "Signal" }
            div { class: "body",
                div { class: "agenda", role: "group", aria_label: "Open the rig",
                    for name in RIGS {
                        button {
                            key: "{name}",
                            class: "ag",
                            aria_label: "Open the {name} rig beside the session",
                            onclick: move |_| open_rig(app, name),
                            span { class: "mid",
                                span { class: "t1", "{name}" }
                                span { class: "s1", "Open the rig beside the session" }
                            }
                        }
                    }
                }
                if folders().is_none() {
                    p { class: "empty", "Looking in Studio's home…" }
                } else if folders().is_some_and(|f| f.is_none()) {
                    super::HomeNotAnswering {}
                } else if folders().flatten().unwrap_or_default().is_empty() {
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
                        for (name , n) in folders().flatten().unwrap_or_default() {
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

/// Opens rig `name`'s screens in the Rig window beside the session, with
/// its audio; a line saying why not when no session is open (or where an
/// app runs no second process: iPhone, iPad, the web).
fn open_rig(app: crate::state::AppState, name: &'static str) {
    #[cfg(all(feature = "native", any(target_os = "linux", target_os = "macos")))]
    if let Some(layout) = crate::stage::RigLayout::parse(name) {
        if crate::hosted::open_rig_window(layout, "") {
            let instrument = (layout == crate::stage::RigLayout::Guitar)
                .then(|| name.to_ascii_lowercase());
            crate::hosted::start_rig(layout, instrument, true);
            app.say(format!("Opened the {name} rig beside the session"));
        } else {
            app.say("The rig opens beside a session: open one first".to_owned());
        }
        return;
    }
    app.say(format!("The {name} rig opens in Studio on a computer"));
}
