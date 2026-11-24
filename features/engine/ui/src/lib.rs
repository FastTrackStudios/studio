//! The Session DAW: the arrangement, the mixer and the panels around them.
//!
//! The arrangement is painted by one thing, [`widget::ArrangementWidget`],
//! mounted by [`panel::use_arrangement_panel`] and [`panel::PanelChrome`]
//! wherever the app shows it — the desktop shell on dioxus-native, the
//! browser through `web_host`. The headless tools (`bin/bench`,
//! `bin/blitz_shot`) draw through the same widget, so a benchmark or a
//! screenshot is a picture of the window rather than of a copy of it.

pub mod animate;
#[cfg(all(feature = "native", target_os = "linux"))]
pub mod app_menu;
pub mod arrange_camera;
pub mod arrange_edit;
pub mod arrangement;
pub mod art;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::automation_actions;
pub use session_engine_client::audio_mode;
pub use session_engine_client::automation;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::automation_actions;
pub mod automation_draw;
pub mod automation_edit;
#[cfg(feature = "native")]
pub mod automation_menu;
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
pub mod comp_predict;
pub mod compact;
pub mod cursor;
pub use session_engine_client::edit_state;
pub mod editor_view;
pub use session_engine_client::engine;
pub mod expression;
pub mod folder_item;
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
pub mod hit;
pub mod icons;
#[cfg(feature = "native")]
pub mod instrument_rail;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::instrument_tracks;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::instrument_tracks;
pub use session_engine_client::jobs;
pub mod keys;
pub mod lane_stack;
pub use session_engine_client::lanes;
pub mod layout;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::live_host;
pub use session_engine_client::leveler;
pub mod loading;
pub mod lyrics_panel;
pub mod marks_strip;
pub use session_engine_client::midi;
pub mod controls;
pub mod dock;
#[cfg(any(feature = "native", feature = "web"))]
pub mod mix_guide;
pub mod mousemap;
pub mod navigator;
pub mod notice;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::open;
pub use session_engine_client::num;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::open;
pub use session_engine_client::open_progress;
pub use session_engine_client::options;
#[cfg(feature = "native")]
pub mod organize;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::organize_edit;
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
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::page;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::page;
pub mod panel;
#[cfg(feature = "native")]
pub mod patch_list;
pub mod performer;
#[cfg(feature = "engine")]
pub use session_engine::poi;
pub use session_engine_client::plan;
#[cfg(not(feature = "engine"))]
pub use session_engine_client::poi;
pub mod poi_paint;
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
pub mod rename;
pub mod replan;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::render_ahead;
#[cfg(all(feature = "native", not(feature = "engine")))]
pub use session_engine_client::render_ahead;
pub mod row;
pub mod ruler;
#[cfg(feature = "engine")]
pub use session_engine::setlist;
#[cfg(not(feature = "engine"))]
pub use session_engine_client::setlist;
pub use session_engine_client::settings;
pub mod setup;
pub mod shell;
pub mod skin;
pub use session_engine_client::song_model;
pub use session_engine_client::stretch;
pub mod strip;
pub mod studio;
pub mod swipe;
#[cfg(all(feature = "native", feature = "engine"))]
pub use session_engine::take_rank;
pub mod take_rating;
pub mod take_window;
pub mod tcp;
#[cfg(test)]
pub(crate) mod test_song;
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
#[cfg(feature = "web")]
pub mod web_host;
pub mod which_key;
#[cfg(feature = "native")]
pub mod window;
pub use session_engine_client::window_queue;
pub mod widget;
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
