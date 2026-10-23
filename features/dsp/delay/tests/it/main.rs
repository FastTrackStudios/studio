//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

#[path = "../support/mod.rs"]
mod support;

#[cfg(all(feature = "ui", feature = "viz"))]
mod families_differ;
#[cfg(all(feature = "native", feature = "ui"))]
mod knob_readouts;
#[cfg(all(feature = "native", feature = "ui"))]
mod param_round_trip;
#[cfg(all(feature = "native", feature = "profiles", feature = "ui"))]
mod screenshots;
mod shifter_quality;
#[cfg(all(feature = "native", feature = "ui"))]
mod tempo_sync;
#[cfg(all(feature = "native", feature = "ui"))]
mod time_field;
