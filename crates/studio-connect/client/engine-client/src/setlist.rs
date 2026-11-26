//! The set as a window shows it (`engine_runtime::client::setlist`,
//! re-exported here), and the page's context that it is only viewed.

pub use engine_runtime::client::setlist::*;

/// A set this page may play but not change: what a guest of a live set
/// gets (the web app, joined by a live share link without signing in —
/// the public demo's playground among them). Provided as context by the
/// page that opened it; the set's editors read it and offer no reorder or
/// removal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadOnlySet(pub bool);

impl ReadOnlySet {
    /// Whether the set on screen is only viewed here.
    #[must_use]
    pub fn here() -> bool {
        dioxus::prelude::try_consume_context::<Self>().is_some_and(|r| r.0)
    }
}
