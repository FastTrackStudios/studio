//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

#[path = "../support/mod.rs"]
mod support;

#[cfg(all(feature = "ui", feature = "viz"))]
mod engines_differ;
#[cfg(all(feature = "native", feature = "profiles", feature = "ui"))]
mod screenshots;
