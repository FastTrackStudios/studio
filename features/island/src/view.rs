//! One of the island's views by its id, given where the song is (`page`:
//! the playhead source's state, flat), the note it is drawn beside
//! (`note`) and a way to ask the playhead (`ask`). The frame build
//! (`crate::web`) and a distribution's web client that links the views
//! in (studio-web's `studio.transport` linked extension) both draw this.

use dioxus::prelude::*;
use serde_json::Value;

use crate::views::{self, playhead_of};

/// View `view` (`progress`, `transport`, `lyrics`, `arrangement`,
/// `home`, `mixer`, `lighting`, `setlist.playlist` or `song`); nothing for
/// another id.
#[component]
pub fn View(view: String, page: Signal<Value>, note: Signal<Value>, ask: views::Ask) -> Element {
    if view == "lyrics" {
        return rsx! { crate::lyrics::Lyrics { page } };
    }
    if view == "arrangement" {
        return rsx! { crate::arrangement::Arrangement { page, ask } };
    }
    if view == "home" {
        return rsx! { crate::home::Home { note, ask } };
    }
    if view == "mixer" {
        return rsx! { crate::arrangement::NotHere {
            title: "Mixer",
            what: "The mixer shows a song's tracks: their levels, pans and effects.",
            why: "This link shares each song's finished mix, not its tracks, so there is nothing to mix here.",
        } };
    }
    if view == "lighting" {
        return rsx! { crate::arrangement::NotHere {
            title: "Lighting Visualizer",
            what: "The lighting view shows a song's cues and looks as it plays.",
            why: "The show's cues are not on this link yet.",
        } };
    }
    if view == crate::queue::VIEW {
        return rsx! { crate::setlist::SetlistView { page, note, ask } };
    }
    if view == crate::song::VIEW {
        return rsx! { crate::chart::SongView { page, note, ask } };
    }
    let p = playhead_of(&page.read());
    match view.as_str() {
        "progress" => rsx! { views::Progress { p, ask } },
        "transport" => rsx! { views::Transport { p, ask } },
        _ => rsx! {},
    }
}
