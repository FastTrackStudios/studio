//! Which Studio this is: `STUDIO_INSTANCE` names a second instance beside
//! the one the person uses (an agent's, a demo's), so the two never share
//! a door or a window rule.
//!
//! Unset or empty, everything is as it always was: the doors are
//! `studio-mcp.json` and `studio.sock`, the engine folder
//! `studio-app-engine`. Set to `agent`, they are `studio-agent-mcp.json`,
//! `studio-agent.sock` and `studio-app-engine-agent`.
//!
//! The window is "Studio" for every instance, app id and title alike: the
//! desktop's window rules (Loft/niri, matching `^Studio$`) apply to each
//! one. The instance shows inside the app, never in the OS window's
//! identity.

use std::sync::OnceLock;

/// The variable that names an instance.
pub const VAR: &str = "STUDIO_INSTANCE";

/// This instance's name, if it has one: letters, digits, `-` and `_` only
/// (it becomes a file name and an app id); anything else is ignored.
pub fn instance() -> Option<&'static str> {
    static NAME: OnceLock<Option<String>> = OnceLock::new();
    NAME.get_or_init(|| {
        let name = std::env::var(VAR).ok()?;
        let ok = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        ok.then_some(name)
    })
    .as_deref()
}

/// The name the app's doors (MCP discovery, control sockets) go by.
pub fn name() -> &'static str {
    static NAME: OnceLock<String> = OnceLock::new();
    NAME.get_or_init(|| match instance() {
        Some(i) => format!("studio-{i}"),
        None => "studio".to_owned(),
    })
}

/// The window's app id: what window rules match. "Studio" for every
/// instance, so the desktop's rules place each one.
#[cfg(feature = "native")]
pub fn app_id() -> String {
    "Studio".to_owned()
}

/// The window's title: "Studio" for every instance (window rules match it
/// too).
#[cfg(feature = "native")]
pub fn title() -> String {
    "Studio".to_owned()
}

/// The engine folder's name under `<runtime dir>/fts`: one engine per
/// instance.
pub fn engine_folder() -> String {
    match instance() {
        Some(i) => format!("studio-app-engine-{i}"),
        None => "studio-app-engine".to_owned(),
    }
}

/// Whether the session is one window (`STUDIO_SINGLE_WINDOW=1`): the
/// view picked fills the main window (the chart alone, the lyrics alone),
/// and the Stage's other windows are not opened. For testing and
/// development on one display.
pub fn single_window() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("STUDIO_SINGLE_WINDOW").is_ok_and(|v| v == "1" || v == "true"))
}
