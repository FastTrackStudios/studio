//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

mod bandpass;
mod engine;
#[cfg(all(feature = "graph", feature = "native", feature = "ui"))]
mod gui_editor;
#[cfg(all(feature = "ui"))]
mod gui_headless;
#[cfg(all(feature = "ui"))]
mod gui_smoke;
mod overheads_preset;
#[cfg(all(feature = "native", feature = "ui"))]
mod param_round_trip;
#[cfg(all(feature = "native", feature = "ui"))]
mod proq_presets;
#[cfg(all(feature = "graph", feature = "graph-paint", feature = "ui"))]
mod scene_snapshot;
#[cfg(all(feature = "native", feature = "ui"))]
mod screenshots;
#[cfg(all(feature = "native", feature = "ui"))]
mod ui_overheads_preset;
