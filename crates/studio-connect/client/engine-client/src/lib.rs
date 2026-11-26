//! What a window needs of Session's engine.
//!
//! The engine (`studio-engine`) runs in a process of its own (`studio
//! engine`, Studio's `daw` extension); a window reaches it over a link
//! (`view_client`, the `EngineView` service in `view`). What both sides
//! share is the engine's (`engine_runtime::client`), so the engine depends
//! on no UI crate; this crate re-exports every module of it from the paths
//! it always had, and adds the few that need Dioxus: the page's
//! [`setlist::ReadOnlySet`] context and the [`song_modes::use_song_modes`]
//! hook.

pub use engine_runtime::client::*;

pub mod setlist;
pub mod song_modes;
