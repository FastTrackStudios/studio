//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

mod pt_playlists_to_lanes;
mod pt_rpp_roundtrip;
mod reaper_action_backend;
mod reaper_action_registry;
mod reaper_arp;
mod reaper_automation;
mod reaper_dawfile_service;
mod reaper_dock_host;
mod reaper_ext_state;
mod reaper_items;
mod reaper_lanes;
mod reaper_multi_daw;
mod reaper_project_isolation;
mod reaper_routing;
mod reaper_ruler;
mod reaper_screenset;
mod reaper_song_files;
mod reaper_take_peaks;
mod reaper_takes;
mod reaper_tempo_map;
mod reaper_timer_health;
mod reaper_toolbar;
mod reaper_track_groups;
mod reaper_track_mirror;
mod reaper_transport;
mod reaper_velocity;
mod reaper_visual_e2e;
mod reaper_window_geometry;
mod rpp_to_ptx_writer;
