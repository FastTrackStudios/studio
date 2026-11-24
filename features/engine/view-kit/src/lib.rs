//! The DAW views' kit: what the arrangement (`arrangement-view`) and the
//! mixer (`mixer-view`) are both drawn with — the theme and its loader
//! (`skin`), the track panel's rows (`tcp`, `row`), the strip controls,
//! text, layout, touch and pointer handling, the ruler's measurements, the
//! dock's links, the window's chrome (`chrome`, `form`) and Action List
//! (`window`). The app's panels around them (the shell, the chart, the
//! lyrics, the transport) are studio-panels'. The arrangement and the mixer
//! install their views into it (`dock::install_arrangement`,
//! `dock::install_mixer`), so nothing here names either one's code.

pub mod animate;
pub mod art;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::automation_actions;
pub use session_engine_client::audio_mode;
pub use session_engine_client::automation;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::automation_actions;
pub mod bar_step;
pub mod beats;
pub mod browser_toggle;
pub use session_engine_client::changes;
pub mod chrome;
#[cfg(all(any(feature = "native", feature = "web"), feature = "engine"))]
pub use session_engine::collab;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::comp;
#[cfg(all(any(feature = "native", feature = "web"), not(feature = "engine")))]
pub use session_engine_client::collab;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::comp;
pub use session_engine_client::edit_state;
pub mod edit_target;
pub use session_engine_client::engine;
pub use session_engine_client::folds;
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
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::instrument_tracks;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::instrument_tracks;
pub use session_engine_client::jobs;
pub mod keys;
pub use session_engine_client::lanes;
pub mod layout;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::live_host;
pub use session_engine_client::leveler;
pub use session_engine_client::midi;
pub mod controls;
pub mod dock;
pub mod mousemap;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::open;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::organize_edit;
pub use session_engine_client::num;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::open;
pub use session_engine_client::open_progress;
pub use session_engine_client::options;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::organize_edit;
pub use session_engine_client::play_return;
pub mod popup;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::actions;
#[cfg(all(not(feature = "native"), feature = "engine"))]
pub use session_engine::open;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::projects;
#[cfg(all(feature = "signal", feature = "engine"))]
pub use session_engine::rig_actions;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::stream_set;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::studio_engine;
#[cfg(feature = "engine")]
pub use session_engine::track_regions;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::actions;
#[cfg(all(not(feature = "native"), not(feature = "engine")))]
pub use session_engine_client::open;
pub use session_engine_client::presence;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::projects;
pub use session_engine_client::song_modes;
pub use session_engine_client::stream_quality;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::stream_set;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::studio_engine;
pub use session_engine_client::take_colors;
#[cfg(not(feature = "engine"))]
pub use session_engine_client::track_regions;
#[cfg(feature = "native")]
pub use session_engine_client::wave_pool;
pub mod device_audio;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::edit_actions;
pub use session_engine_client::drum_edit;
pub use session_engine_client::drum_settings;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::edit_actions;
pub use session_engine_client::gain_stage;
pub mod grid_in_force;
pub use session_engine_client::hit_overlay;
pub use session_engine_client::kit_gate;
pub mod overlay;
pub mod palette;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::page;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::page;
pub mod panel_input;
#[cfg(feature = "engine")]
pub use session_engine::poi;
pub use session_engine_client::plan;
#[cfg(not(feature = "engine"))]
pub use session_engine_client::poi;
pub mod pointer;
#[cfg(feature = "engine")]
pub use session_engine::prepare;
#[cfg(not(feature = "engine"))]
pub use session_engine_client::prepare;
#[cfg(feature = "native")]
pub mod perf_meter;
pub mod profile;
#[cfg(feature = "native")]
pub mod quantize_panel;
pub mod rails;
pub mod replan;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::render_ahead;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::render_ahead;
pub mod row;
pub mod ruler_layout;
#[cfg(feature = "engine")]
pub use session_engine::setlist;
#[cfg(not(feature = "engine"))]
pub use session_engine_client::setlist;
pub use session_engine_client::settings;
pub mod skin;
pub mod song_left;
pub use session_engine_client::song_model;
pub use session_engine_client::stretch;
pub mod strip;
pub mod studio;
pub mod take_rating;
pub mod tcp;
/// A small song for tests, here and in the crates over this one (their
/// dev-dependency turns on `test-support`).
#[cfg(any(test, feature = "test-support"))]
pub mod test_song;
pub mod text;
pub mod tool;
pub mod toolbar;
pub mod touch;
pub mod track_icon;
#[cfg(all(feature = "web", feature = "engine"))]
pub use session_engine::web_engine;
#[cfg(feature = "native")]
pub use session_engine_client::view_client;
pub use session_engine_client::wave_detail;
#[cfg(feature = "web")]
pub use session_engine_client::web_audio;
pub mod press;
pub mod view_actions;
pub mod viewport;
#[cfg(feature = "web")]
pub mod web_canvas;
pub mod which_key;
#[cfg(feature = "native")]
pub mod window;
pub use session_engine_client::window_queue;
pub mod window_cursor;
pub use session_engine_client::zoom;

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
