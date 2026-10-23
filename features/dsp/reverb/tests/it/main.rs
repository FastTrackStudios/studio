//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

#[path = "../support/mod.rs"]
mod support;

mod cloud_character;
mod conv_mod;
mod engine_params;
#[cfg(all(feature = "ui", feature = "viz"))]
mod families_differ;
mod generator_params;
mod impulse_params;
#[cfg(all(feature = "native", feature = "profiles", feature = "ui"))]
mod ir_browser;
mod ir_metrics;
mod ir_samples;
#[cfg(all(feature = "native", feature = "profiles", feature = "ui"))]
mod knob_readouts;
#[cfg(all(feature = "native", feature = "ui"))]
mod param_round_trip;
mod pattern_lanes;
#[cfg(all(feature = "native"))]
mod presets;
#[cfg(all(feature = "native", feature = "profiles", feature = "ui"))]
mod screenshots;
mod stability;
#[cfg(all(feature = "native", feature = "ui"))]
mod tempo_sync;
mod true_stereo;
mod voice_params;
