//! A room's link opened in the app (`/join/<room>`, the room's lasting
//! link on fasttrackstudio.app): the room it names, and the way into it
//! from here. Signed out, the landing page shows the same room
//! (`landing::RoomDoor`); signed in, this page.
//!
//! The room itself is joined by Studio's engine: on a computer, iPhone or
//! iPad, through the app's own link (`fasttrackstudio://join/<room>`). A
//! browser following the room in the page is studio#711's.

use dioxus::prelude::*;

use crate::pieces::{Bar, Page};

#[component]
pub fn JoinRoom(room: String) -> Element {
    let Some(room) = crate::landing::room_in(&room) else {
        return rsx! {
            Page {
                Bar { title: "Room" }
                div { class: "body",
                    p { class: "s1", role: "alert", "That isn't a room's link." }
                }
            }
        };
    };
    let app = crate::landing::app_link(&room);
    rsx! {
        Page {
            Bar { title: "Room" }
            div { class: "body", "data-room": "{room}",
                h2 { "{room}" }
                p { "Open this room in Studio on your computer, iPhone or iPad: whatever its leader loads, you follow." }
                a { class: "primary wide", href: "{app}", "Open in Studio" }
            }
        }
    }
}
