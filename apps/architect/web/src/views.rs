//! The engine's panels behind the island's tabs: the same Arrangement
//! and Mixer a Session page draws (`session_ui::web_host`), over the song
//! the engine has current.

use dioxus::prelude::*;
use session_ui::web_host::{HostedPanel, HostedView};

/// The compiled Tailwind the panels are styled with: the desktop app's
/// sheet (as `session-daw-web`).
const TAILWIND: &str = include_str!("../../../session/desktop/assets/tailwind-signal.css");

/// Whether the engine has a session to draw (open, or opening): without
/// one (a link that shares no project, an engine that did not open) the
/// island's own view is shown, as in a build with no engine.
pub fn has_engine() -> bool {
    use session_ui::web_host::HostedState;
    matches!(
        session_ui::web_host::hosted(),
        HostedState::Opening(_) | HostedState::Open(_)
    )
}

/// The Arrangement tab: tracks, items, sections and markers.
pub fn arrangement() -> Element {
    rsx! {
        style { {TAILWIND} }
        HostedView { panel: HostedPanel::Arrangement }
    }
}

/// The Mixer tab: mute, solo and levels, this listener's own.
pub fn mixer() -> Element {
    rsx! {
        style { {TAILWIND} }
        HostedView { panel: HostedPanel::Mixer }
    }
}
