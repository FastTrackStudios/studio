//! The browser's place on the window's Action List: its action
//! ([`BROWSER`], `b`) and the toggles asked for there, which the browser
//! panel (studio-panels' `browser`) carries out. The window registers the
//! action (`crate::window`); the panel is the app's.

/// Show or hide the browser: what to load on the selected track.
#[cfg(feature = "native")]
pub static BROWSER: architect::action::ActionMeta = crate::actions::meta(
    "SESSION_BROWSER",
    "browser",
    "Browser",
    "Show or hide the browser down the window's left: what to load on the selected track — signal's rig on a guitar or a bass (its profiles, its stacks' patches: a click plays one, a Shift-click switches to it at the cursor).",
    "View",
    "B",
);

/// Toggles asked for on the Action List and not yet carried out.
static ASKED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Ask the window to show or hide the browser ([`BROWSER`]).
pub fn request_toggle() {
    ASKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// The toggles asked for since the last call, taken: the browser panel
/// shows or hides itself once for an odd count.
pub fn take_toggles() -> u32 {
    ASKED.swap(0, std::sync::atomic::Ordering::Relaxed)
}
