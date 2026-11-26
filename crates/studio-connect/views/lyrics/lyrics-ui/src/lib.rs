//! Session's lyrics panel and the model it draws, on their own: Dioxus
//! and nothing of Session's engine, so the share page's island (studio's
//! web bundle, `features/island`) mounts the very panel Session's
//! window and the setlists song workspace do.
//!
//! - [`lyrics`]: a song's lyrics in layers on its timeline, placed against
//!   its sections and broken into slides (`session::lyrics` re-exports it).
//! - `view` (feature `view`): the panel, in its three views (`session::ui::components::lyrics`
//!   re-exports it).

// The model is studio-core's (`lyrics`), so the engine reads it without
// this crate; re-exported here so the panel's callers keep their paths.
pub use ::lyrics;
// `view` came from Session as it was, and its lint debt with it
// (Session's crate is in no clippy gate; this one is, through the
// island's): paid down on its own, not in the move. New code here keeps
// to the workspace's lints.
#[cfg(feature = "view")]
#[allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_precision_loss,
    clippy::redundant_clone,
    clippy::suboptimal_flops,
    clippy::doc_markdown,
    clippy::derive_partial_eq_without_eq,
    clippy::type_complexity,
    clippy::missing_const_for_fn,
    clippy::or_fun_call,
    clippy::needless_collect
)]
pub mod view;
