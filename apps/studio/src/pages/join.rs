//! A room's link opened in the app (`/join/<room>`, the room's lasting
//! link on fasttrackstudio.app): the room it names, and the way into it
//! from here. Signed out, the landing page shows the same room
//! (`landing::RoomDoor`); signed in, this page.
//!
//! On a computer, iPhone or iPad the room is joined by Studio's engine,
//! through the app's own link (`fasttrackstudio://join/<room>`). In a
//! browser the page itself joins and follows the leader (studio#711,
//! `crate::room_web`): its person presses Join, the gesture a browser
//! needs before it plays the room's sound.

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
                InThePage { room: room.clone() }
                p { "Open this room in Studio on your computer, iPhone or iPad: whatever its leader loads, you follow." }
                a { class: "primary wide", href: "{app}", "Open in Studio" }
            }
        }
    }
}

/// In a browser: the room, who leads it and what is loaded, and Join.
#[cfg(feature = "web")]
#[component]
fn InThePage(room: String) -> Element {
    use crate::room_web::{JOIN, SHOWN};
    let Ok(id) = room::parse_link(&room) else {
        return rsx! {
            h2 { "{room}" }
        };
    };
    let s = SHOWN();
    let joined = JOIN().as_ref() == Some(&id);
    let title = if s.name.is_empty() {
        id.to_string()
    } else {
        s.name.clone()
    };
    let line = match s.state {
        _ if !joined => {
            "Join to follow the leader here: what they load opens in this page, and plays with them."
                .to_owned()
        }
        "finding" => "Finding the room…".to_owned(),
        "joining" if s.why.is_empty() => format!("Reaching {}…", s.leader),
        "joining" => format!("Not in the room: {}", s.why),
        "member" if s.loaded.is_empty() => {
            format!("In the room. {} has loaded nothing yet.", s.leader)
        }
        "member" if s.role == "guest" => format!(
            "Following {} as a guest ({}): {}. A guest plays along and edits nothing.",
            s.leader, s.me, s.loaded
        ),
        "member" => format!("Following {}: {}", s.leader, s.loaded),
        "practising" => format!("Nobody leads it now: {} is open to practise on your own.", s.loaded),
        "failed" => format!("Could not join: {}", s.why),
        _ => String::new(),
    };
    let people = s.people.join(", ");
    rsx! {
        h2 { "{title}" }
        p { class: "pline", "{line}" }
        if joined && !people.is_empty() {
            p { class: "pline", "Here: {people}" }
        }
        if joined && s.state == "member" && !s.why.is_empty() {
            div { class: "s1", role: "alert", "{s.why}" }
        }
        if joined {
            button {
                id: "room-leave",
                class: "primary",
                onclick: move |_| *JOIN.write() = None,
                "Leave"
            }
        } else {
            button {
                id: "room-join",
                class: "primary wide",
                onclick: move |_| *JOIN.write() = Some(id.clone()),
                "Join in this browser"
            }
        }
    }
}

/// Outside a browser: the room's name; Studio's engine joins it.
#[cfg(not(feature = "web"))]
#[component]
fn InThePage(room: String) -> Element {
    rsx! {
        h2 { "{room}" }
    }
}
