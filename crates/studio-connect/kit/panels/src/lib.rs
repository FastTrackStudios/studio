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
// Studio's own half of what a window is built from (the kit is any DAW's),
// under the paths the panels were written against: the engine's, where
// it runs in this process (`engine`), else the window's.
#[cfg(all(any(feature = "native", feature = "web"), not(feature = "engine")))]
#[allow(unused_imports)]
use ::engine_client::collab;
/// The window's mode follows the song (a Dioxus hook, so the window's,
/// not the engine's): for a shell over its setlist and its mode.
pub use ::engine_client::song_modes::use_song_modes;
#[cfg(feature = "native")]
#[allow(unused_imports)]
use ::engine_client::view_client;
#[cfg(feature = "web")]
#[allow(unused_imports)]
use ::engine_client::web_audio;
#[cfg(not(feature = "engine"))]
#[allow(unused_imports)]
use ::engine_client::{
    actions, instrument_tracks, open, organize_edit, prepare, projects, setlist, stream_set,
};
#[allow(unused_imports)]
use ::engine_client::{audio_mode, song_model, song_modes, stream_quality};
#[cfg(all(feature = "native", not(feature = "engine")))]
#[allow(unused_imports)]
use ::engine_client::{render_ahead, studio_engine};
#[cfg(all(any(feature = "native", feature = "web"), feature = "engine"))]
#[allow(unused_imports)]
use ::studio_engine::collab;
#[cfg(any(feature = "web", feature = "engine"))]
#[allow(unused_imports)]
use ::studio_engine::live_host;
#[cfg(all(feature = "signal", feature = "engine"))]
#[allow(unused_imports)]
use ::studio_engine::rig_actions;
#[cfg(feature = "web")]
#[allow(unused_imports)]
use ::studio_engine::web_engine;
#[cfg(feature = "engine")]
#[allow(unused_imports)]
use ::studio_engine::{
    actions, instrument_tracks, open, organize_edit, prepare, projects, setlist, stream_set,
};
#[cfg(all(feature = "native", feature = "engine"))]
#[allow(unused_imports)]
use ::studio_engine::{render_ahead, studio_engine};

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
pub mod hooks;
#[cfg(feature = "native")]
pub mod instrument_rail;
pub mod key_profile;
pub mod loading;
pub mod lyrics_panel;
#[cfg(any(feature = "native", feature = "web"))]
pub mod mix_guide;
pub mod navigator;
#[cfg(feature = "native")]
pub mod organize;
/// The Patch List view: the album's plan in the room it is played in
/// (moved from arrangement-view: a Studio feature, not the arrangement's).
#[cfg(feature = "native")]
pub mod patch_list;
pub mod performer;
pub mod progress;
pub mod record_view;
pub mod setup;
pub mod shell;
pub mod shows;
pub mod transport_bar;
#[cfg(feature = "native")]
pub mod versions;
#[cfg(feature = "web")]
pub mod web_host;
