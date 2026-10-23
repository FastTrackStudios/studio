//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

mod scene_rows;
#[cfg(all(feature = "reaper"))]
#[path = "01_no_unneccesary_folders.rs"]
mod t_01_no_unneccesary_folders;
#[cfg(all(feature = "reaper"))]
#[path = "02_scoped_resorting.rs"]
mod t_02_scoped_resorting;
#[cfg(all(feature = "reaper"))]
#[path = "03_assertion_sensitivity.rs"]
mod t_03_assertion_sensitivity;
#[cfg(all(feature = "reaper"))]
#[path = "04_visibility_classification.rs"]
mod t_04_visibility_classification;
#[cfg(all(feature = "reaper"))]
#[path = "06_organize_is_lossless.rs"]
mod t_06_organize_is_lossless;
#[cfg(all(feature = "reaper"))]
#[path = "07_template_group_flags.rs"]
mod t_07_template_group_flags;
#[cfg(all(feature = "reaper"))]
#[path = "08_golden_session.rs"]
mod t_08_golden_session;
#[cfg(all(feature = "reaper"))]
#[path = "08_scene_selector.rs"]
mod t_08_scene_selector;
#[cfg(all(feature = "reaper"))]
#[path = "09_grouping_watcher.rs"]
mod t_09_grouping_watcher;
