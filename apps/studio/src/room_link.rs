//! Joining a room from its link (ADR 0003): the link a band keeps, given
//! to Studio, and the engine follows that room's leader from then on.
//!
//! A link reaches the app two ways:
//! - **opened** on iPhone or iPad: `fasttrackstudio://join/<room>`, the
//!   URL type the iOS build declares, tapped anywhere (a message, a QR
//!   code), launches Studio or brings it up (studio-shell's
//!   `ios_scene::on_link`);
//! - **at launch**, on any native build: `Studio --join <link>`.
//!
//! Either way it is the engine that joins (`ROOM_JOIN`, through
//! [`crate::engine::join_room`]): what the leader loads opens in it, and
//! its transport keeps to theirs. On a phone that engine is the one linked
//! into the app, and it finds the room in the app's own Studio folder
//! ([`rooms_dir`]).

use std::path::{Path, PathBuf};

/// The link `args` (the program's arguments, its name first) ask Studio to
/// join: `--join <link>` or `--join=<link>`.
pub fn link_in(args: &[String]) -> Option<String> {
    let mut rest = args.iter().skip(1);
    while let Some(arg) = rest.next() {
        let link = if arg == "--join" {
            rest.next().cloned()
        } else {
            arg.strip_prefix("--join=").map(str::to_owned)
        };
        if let Some(link) = link.filter(|l| !l.trim().is_empty()) {
            return Some(link);
        }
    }
    None
}

/// Whether `link` names a room: anything else opened by the app (another
/// kind of link) is not this module's.
pub fn is_room_link(link: &str) -> bool {
    room::parse_link(link).is_ok()
}

/// Where the engine linked into the app keeps and finds rooms: the app's
/// own Studio folder's `Rooms`, beside its Setlists and Songs, so a room
/// and what it loads mean the same folder (`FTS_ROOMS_DIR`, which the
/// engine reads). `None` without a Studio folder.
pub fn rooms_dir(home: Option<&Path>) -> Option<PathBuf> {
    home.map(|h| h.join("Rooms"))
}

/// What the room shows this device as: the device's own word on a phone or
/// tablet ("iPhone", "iPad"), "Studio" elsewhere. On the main thread: UIKit
/// is asked.
pub fn member_name() -> String {
    #[cfg(target_os = "ios")]
    let name = studio_shell::ios_scene::device_name();
    #[cfg(not(target_os = "ios"))]
    let name = "Studio";
    name.to_owned()
}

/// Join the room `link` names as `name` ([`member_name`]), off the UI
/// thread: the engine is linked to (or started) first, as playing does.
/// How it went is the `studio.room.join` span's (`engine::join_room`);
/// `ROOM` (the engine's) says how following goes. A link that names no
/// room is not this module's and is left alone.
pub fn join(link: String, name: String) {
    if !is_room_link(&link) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("studio-room-join".to_owned())
        .spawn(move || {
            // The outcome and its error are the span's.
            let _ = crate::engine::join_room(&link, &name);
        });
    if let Err(e) = spawned {
        tracing::error!(room.error = %e, "room: no thread to join the room on");
    }
}

/// studio-shell's handler for a link the app is opened with (on the main
/// thread).
#[cfg(target_os = "ios")]
pub fn link_opened(link: &str) {
    join(link.to_owned(), member_name());
}

#[cfg(test)]
mod tests;
