//! Studio's app panels around the DAW views: the shell and its bars
//! (`shell`, `compact`, `transport_bar`), the chart (`chart_panel`,
//! `chart_editor`), the lyrics, the performer screen, the setlist's
//! navigator and progress, collaboration (`collab_bar`, `collab_pointers`),
//! Setup, the record view, the browser, versions, the loading screen, the
//! web page (`web_host`) and the desktop's app menu (`app_menu`).
//!
//! They were daw-view-kit's; they are the app's, so they live here and the
//! kit stays a kit (daw-view-kit/docs/extraction-plan.md, step 3). What the
//! views need of the app's chrome is the kit's (`chrome`, `form`,
//! `web_canvas`, `browser_toggle`): the app provides it as contexts, and
//! the modules here re-export it under the paths they always had.

// The kit, under the paths this code was written against (`crate::tcp`,
// `crate::actions`, …).
#[allow(unused_imports)]
use daw_view_kit::*;

#[cfg(all(feature = "native", target_os = "linux"))]
pub mod app_menu;
pub mod browser;
/// The chart editor, editing the song's chart on the engine in this
/// process (`prepare::apply_chart`, the live session's chart): the old
/// Session window's.
#[cfg(all(feature = "native", feature = "engine"))]
pub mod chart_editor;
pub mod chart_panel;
#[cfg(any(feature = "native", feature = "web"))]
pub mod collab_bar;
#[cfg(feature = "native")]
pub mod collab_pointers;
pub mod compact;
pub mod loading;
pub mod lyrics_panel;
#[cfg(any(feature = "native", feature = "web"))]
pub mod mix_guide;
pub mod navigator;
pub mod performer;
pub mod progress;
pub mod record_view;
pub mod setup;
pub mod shell;
pub mod transport_bar;
#[cfg(feature = "native")]
pub mod versions;
#[cfg(feature = "web")]
pub mod web_host;
