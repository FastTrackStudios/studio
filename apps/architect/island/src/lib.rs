//! Studio's island (studio#445): Session's song progress bar and transport
//! controls (`transport_ui`, the same components Session draws), on a share
//! link's now-playing view.
//!
//! Its two views are mounted by the page's player at the top of the view
//! (`progress`) and under the chart (`transport`); its web bundle has a
//! third, the Lyrics tab's (`lyrics`: Session's lyrics panel, fixed to its
//! Confidence Monitor, given the song's timed lines too). Each is given, as its
//! props, where the song is: `time`, `playing`, `duration`, `title`, and
//! its `sections` (`[{name, start, end, color}]`, seconds). It asks the
//! page only `PLAYHEAD_SEEK` (`{t}`), `PLAYHEAD_PLAY`, `PLAYHEAD_PAUSE`,
//! `PLAYHEAD_NEXT`, `PLAYHEAD_PREVIOUS`.
//!
//! Built two ways, the same views ([`views`]):
//! - for `wasm32-wasip2`, a WASI component (`just island`) the page's
//!   sandbox runs with Architect's web runtime (a browser with JS Promise
//!   Integration);
//! - for `wasm32-unknown-unknown`, a web bundle (`just island-web`, its
//!   `mount(view)`) the page runs in a web view frame of its own
//!   (Architect's `SiteIsland::web`): any browser, Safari before 27 and
//!   every browser on iOS included.
//!
//! Empty anywhere else.
#![cfg(target_arch = "wasm32")]

pub mod views;

#[cfg(target_os = "wasi")]
mod wasi;

#[cfg(target_os = "unknown")]
mod lyrics;

#[cfg(target_os = "unknown")]
mod web;
