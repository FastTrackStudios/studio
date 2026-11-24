//! The DAW views' kit: what the arrangement (`arrangement-view`) and the
//! mixer (`mixer-view`) are both drawn with — the theme and its loader
//! (`skin`), the track panel's rows (`tcp`, `row`), the strip controls,
//! text, layout, touch and pointer handling, the ruler's measurements, the
//! dock's links, the window's chrome (`chrome`, `form`) and Action List
//! (`window`). The app's panels around them (the shell, the chart, the
//! lyrics, the transport) are studio-panels'. The arrangement and the mixer
//! install their views into it (`dock::install_arrangement`,
//! `dock::install_mixer`), so nothing here names either one's code.

#[cfg(any(feature = "native", feature = "web"))]
pub use daw_view::arrange_actions;
/// The views' contract (`daw-view`), under the paths the views were
/// written against (`crate::engine`, `crate::view`, …).
pub use daw_view::{
    actions, automation, automation_actions, body, changes, edit_actions, edit_state, engine,
    folds, hit_overlay, host, jobs, kit_gate, lanes, leveler, midi, num, open, open_progress,
    options, plan, play_return, poi, routes, scale, settings, stretch, track_regions, view, wake,
    wave_detail, window_queue,
};
#[cfg(feature = "native")]
pub use daw_view::{notes_actions, page, view_client, wave_pool};
pub mod animate;
pub mod app;
pub mod art;
pub mod bar_step;
pub mod beats;
pub mod browser_toggle;
#[cfg(any(test, feature = "test-support"))]
pub mod bypass_guard;
pub mod chrome;
pub mod controls;
pub mod dock;
pub mod edit_target;
pub mod form;
pub mod fps;
pub mod frames;
pub mod gesture;
pub mod ghosts;
/// What the window's GPU device asks for: buffers as large as the adapter has.
#[cfg(feature = "native")]
pub mod gpu;
#[cfg(feature = "native")]
pub mod headless;
pub mod icons;
pub mod key_track;
pub mod keys;
pub mod layout;
pub mod mouse_context;
pub mod mousemap;
pub mod popup;
/// Presence held for the process: the session primitive's store.
pub use fts_session::presence::store as presence;
pub mod device_audio;
pub mod grid_in_force;
pub mod overlay;
pub mod page_engine;
pub mod palette;
pub mod panel_input;
#[cfg(feature = "native")]
pub mod perf_meter;
pub mod pointer;
pub mod press;
pub mod profile;
#[cfg(feature = "native")]
pub mod quantize_panel;
pub mod rail;
pub mod replan;
pub mod row;
pub mod ruler_layout;
pub mod shows;
pub mod skin;
pub mod song_left;
pub mod song_model;
pub mod strip;
pub mod studio;
pub mod take_colors;
pub mod take_rating;
pub mod tcp;
/// A small song for tests, here and in the crates over this one (their
/// dev-dependency turns on `test-support`).
#[cfg(any(test, feature = "test-support"))]
pub mod test_song;
pub mod text;
pub mod theme_tokens;
pub mod tool;
pub mod toolbar;
pub mod touch;
pub mod track_icon;
pub mod view_actions;
pub mod viewport;
#[cfg(feature = "web")]
pub mod web_canvas;
pub mod which_key;
#[cfg(feature = "native")]
pub mod window;
pub mod window_cursor;
pub mod zoom;

/// Whether this process's windows are linked into the app that hosts them
/// (`session_engine::actions::in_host`).
#[cfg(feature = "native")]
#[must_use]
pub fn engine_actions_in_host() -> bool {
    crate::actions::is_in_host()
}

/// Wait `dur`, on whatever drives this crate's futures: a timer of its own
/// on a desktop (the app that hosts these components may run them outside
/// any tokio runtime: an Architect window does), the platform's elsewhere.
pub async fn pause(dur: std::time::Duration) {
    #[cfg(feature = "native")]
    futures_timer::Delay::new(dur).await;
    #[cfg(not(feature = "native"))]
    architect::platform::sleep(dur).await;
}
