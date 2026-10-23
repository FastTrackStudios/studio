//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

#[path = "../common/mod.rs"]
mod common;

#[cfg(all(feature = "server"))]
mod antelope_fake_server;
#[cfg(all(feature = "server"))]
mod chanmap;
#[cfg(all(feature = "server"))]
mod engine_sandbox;
#[cfg(all(feature = "server"))]
mod fake_server;
#[cfg(all(feature = "server"))]
mod model;
#[cfg(all(feature = "server"))]
mod protocol;
#[cfg(all(feature = "server"))]
mod rcp;
#[cfg(all(feature = "server"))]
mod table;
