//! What a Studio view extension's process is: a Studio window on Studio's
//! engine.
//!
//! It attaches to the engine the `daw` extension runs exactly as
//! `studio-desktop`'s window attaches to a `studio engine`
//! (`session_engine_client::studio_engine::link`, `session_engine_client::open::attach`)
//! ([`attach`]), reads the song through the window protocol's replicas
//! (`session_engine_client::view_client`) ([`use_song`]), runs the engine's
//! actions over that link ([`run`]), and hands Architect's pane input on
//! as the Blitz events Studio's widgets take ([`input`]). A view's pane
//! paints its widget as one layer that crosses only when it changed
//! ([`SceneLayer`]).
//!
//! `studio.arrangement` and `studio.mixer` both build on it; neither
//! needs the other installed.
//!
//! **In a browser** (`wasm32`, studio-web) the engine is in the page: the
//! `daw` extension's browser build opens it (`session_engine::web_engine`,
//! hosted by `daw_view_kit::web_host`) and this crate reads the song it has
//! current from there ([`current`], [`use_song`]) and runs the page's
//! Action List ([`call`]). The paint panes, the input hand-over and the
//! engine's value table are the native process's alone.

#[cfg(not(target_arch = "wasm32"))]
pub mod input;
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
#[cfg(target_arch = "wasm32")]
pub use web::*;

use dioxus::prelude::*;
use serde_json::Value as Json;

/// What a pane shows until there is a song: what it waits for.
///
/// # Errors
///
/// None: it renders.
pub fn waiting() -> Element {
    let surface = surface();
    let why = problem().unwrap_or_else(|| "Waiting for Studio's engine to open a song…".to_owned());
    rsx! {
        div { style: "padding:16px; font-size:13px; opacity:0.7; color:#ccc; background:{surface}; height:100%;",
            "{why}"
        }
    }
}

/// The panels' background, Studio's dark theme's (the session window is
/// dark only).
#[must_use]
pub fn surface() -> String {
    daw_ui::studio::lanes::Colors::from_theme(&daw_ui::theming::Theme::dark()).surface
}

/// The class a view's root takes for Studio's box sizing.
///
/// Studio's window sizes every box by its border (`*{box-sizing:
/// border-box}`, `studio-desktop`'s root style). A view's root takes this
/// class, and [`studio_style`] gives it the rule: the view's own, not the
/// Architect window's around it.
pub const ROOT_CLASS: &str = "studio-view";

/// The rule [`ROOT_CLASS`] names, to render once in a view.
///
/// A `style` element in the view's own tree. A process view is a Blitz
/// sub-document
/// of the host's (`architect_extensions::remote`), which takes a `style`
/// element as a sheet; `document::Style` goes to a document provider the
/// guest has none of.
///
/// # Errors
///
/// None: it renders.
pub fn studio_style() -> Element {
    rsx! {
        style { {".studio-view, .studio-view * { box-sizing: border-box; }"} }
    }
}

/// The window actions the mixer pane carries out (`studio.mixer` claims
/// them from the engine; a window showing the arrangement hands them
/// over): what `studio-desktop`'s one window runs itself.
pub const MIXER_ACTIONS: [&str; 4] = [
    "MIXER_VIEW",
    "SESSION_MIXER_FX",
    "SESSION_TOGGLE_MIXER",
    "SESSION_CLOSEUP",
];

/// How a pane has been painting.
#[derive(Debug, Default, Clone)]
pub struct Stats {
    pub frames: u64,
    /// Frames whose scene differed from the last (and so crossed).
    pub changed: u64,
    pub size: (u32, u32),
    pub commands: usize,
    /// The view's work a frame (the panel's frame and the widget's
    /// paint), the latest few hundred, in milliseconds.
    pub work_ms: std::collections::VecDeque<f64>,
}

impl Stats {
    /// As an action answers it.
    #[must_use]
    pub fn json(&self) -> Json {
        let pct = |p: usize| {
            let mut s: Vec<f64> = self.work_ms.iter().copied().collect();
            s.sort_by(f64::total_cmp);
            s.get(
                s.len()
                    .saturating_sub(1)
                    .saturating_mul(p)
                    .checked_div(100)
                    .unwrap_or(0),
            )
            .copied()
            .unwrap_or(0.0)
        };
        serde_json::json!({
            "count": self.frames,
            "changed": self.changed,
            "size": [self.size.0, self.size.1],
            "commands": self.commands,
            "work_p50_ms": pct(50),
            "work_p99_ms": pct(99),
        })
    }
}
