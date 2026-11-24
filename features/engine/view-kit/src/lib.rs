//! The DAW views' kit: what the arrangement (`arrangement-view`) and the
//! mixer (`mixer-view`) are both drawn with — the theme and its loader
//! (`skin`), the track panel's rows (`tcp`, `row`), the strip controls,
//! text, layout, touch and pointer handling, the ruler's measurements, the
//! dock's links — and, for now, the panels around them (the shell, the
//! chart, the lyrics, the transport). The arrangement and the mixer
//! install their views into it (`dock::install_arrangement`,
//! `dock::install_mixer`), so nothing here names either one's code.

pub mod animate;
#[cfg(all(feature = "native", target_os = "linux"))]
pub mod app_menu;
pub mod art;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::automation_actions;
pub use session_engine_client::audio_mode;
pub use session_engine_client::automation;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::automation_actions;
pub mod bar_step;
pub mod beats;
/// The chart editor, editing the song's chart on the engine in this
/// process (`prepare::apply_chart`, the live session's chart): the old
/// Session window's.
#[cfg(all(feature = "native", feature = "engine"))]
pub mod chart_editor;
pub use session_engine_client::changes;
pub mod chart_panel;
#[cfg(all(any(feature = "native", feature = "web"), feature = "engine"))]
pub use session_engine::collab;
#[cfg(all(any(feature = "native", feature = "web"), not(feature = "engine")))]
pub use session_engine_client::collab;
#[cfg(any(feature = "native", feature = "web"))]
pub mod collab_bar;
#[cfg(feature = "native")]
pub mod collab_pointers;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::comp;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::comp;
pub mod compact;
pub use session_engine_client::edit_state;
pub mod editor_view;
pub use session_engine_client::engine;
pub mod expression;
pub use session_engine_client::folds;
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
pub mod loading;
pub mod lyrics_panel;
pub use session_engine_client::midi;
pub mod controls;
pub mod dock;
#[cfg(any(feature = "native", feature = "web"))]
pub mod mix_guide;
pub mod mousemap;
pub mod navigator;
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
pub use session_engine::projects;
#[cfg(all(feature = "signal", feature = "engine"))]
pub use session_engine::rig_actions;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::stream_set;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::studio_engine;
#[cfg(feature = "engine")]
pub use session_engine::track_regions;
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
pub mod versions;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::actions;
#[cfg(all(not(feature = "native"), feature = "engine"))]
pub use session_engine::open;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::vocal_level;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::actions;
#[cfg(all(not(feature = "native"), not(feature = "engine")))]
pub use session_engine_client::open;
#[cfg(feature = "native")]
pub use session_engine_client::wave_pool;
pub mod browser;
pub mod device_audio;
#[cfg(all(feature = "signal", feature = "engine"))]
pub use session_engine::drum_samples;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::edit_actions;
pub use session_engine_client::drum_edit;
pub use session_engine_client::drum_settings;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::edit_actions;
pub use session_engine_client::gain_stage;
pub mod grid_in_force;
pub use editing::groove;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::mix_chains;
pub use session_engine_client::hit_overlay;
pub use session_engine_client::kit_gate;
pub mod overlay;
pub mod palette;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::page;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::page;
pub mod panel_input;
pub mod performer;
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
pub mod progress;
#[cfg(feature = "native")]
pub mod quantize_panel;
pub mod rails;
pub mod record_view;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::reference_play;
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
pub mod setup;
pub mod shell;
pub mod skin;
pub mod song_left;
pub use session_engine_client::song_model;
pub use session_engine_client::stretch;
pub mod strip;
pub mod studio;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::take_rank;
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
pub mod transport_bar;
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
pub mod web_host;
pub mod which_key;
#[cfg(feature = "native")]
pub mod window;
pub use session_engine_client::window_queue;
pub mod window_cursor;
pub use editing::written;
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
