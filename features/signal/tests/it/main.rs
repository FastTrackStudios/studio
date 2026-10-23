//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

mod fixtures;
mod reaper_all_around_profile;
mod reaper_build_profiles;
mod reaper_clap_proq4_params;
mod reaper_compression_macro;
mod reaper_eq_block_variations;
mod reaper_fabfilter_load;
mod reaper_fast_path_open;
mod reaper_guitar_rig_save;
mod reaper_guitar_setlist;
mod reaper_macro_learn;
mod reaper_macro_setup;
mod reaper_macros_bootstrap;
mod reaper_module_capture;
mod reaper_multi_fx_macro;
mod reaper_nam_amp_presets;
mod reaper_nam_load;
mod reaper_ndsp_param_inventory;
mod reaper_profile_update_roundtrip;
mod reaper_registered_actions;
mod reaper_rig_structure;
mod reaper_save_track_template;
mod reaper_scene_switching;
mod reaper_signal_extension;
mod reaper_signal_load;
mod reaper_worship_rig;
mod signal_builder_api;
mod signal_domain;
mod signal_guitar_rig_api;
mod signal_keys_engine_api;
mod signal_live_runtime;
mod signal_patches_songs;
mod signal_rack_api;
mod signal_setlists_browser;
mod test_file;
