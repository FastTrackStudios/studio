//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

#[path = "../support/mod.rs"]
mod support;

#[cfg(all(feature = "reaper-theme"))]
mod antitheme;
#[cfg(all(feature = "reaper-theme"))]
mod corpus;
mod folders;
mod fx_button;
mod meters;
mod mixer_strip;
mod mute_button;
mod native_panels;
mod replay;
mod strip_controls;
mod strip_shot;
mod traced_art;
mod volume_fader;
mod window_shot;
