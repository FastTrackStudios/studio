//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

mod drum_material;
mod greenfield_compat;
// It mounts trigger::ui, so it needs `ui` as well as `native` (its old
// `required-features` only said `native`, and passed on feature unification).
#[cfg(all(feature = "native", feature = "ui"))]
mod gui_editor;
mod trigger;
