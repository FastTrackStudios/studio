//! Session's host-neutral runtime: the part of its engine that every run
//! mode needs — the native app, the web bundle, and an Architect-hosted
//! extension — with no window, no shell and no Dioxus.
//!
//! - [`source`] — where a streamed song's files come from (`SongSource`),
//!   and a Task share link as one.
//! - [`song_stream`] — a song streamed from a source: its small files
//!   mirrored, its proxies fetched in the order they will be heard, on the
//!   host's runtime ([`song_stream::Spawn`]).
//! - [`listen`] — how a song is heard, and the cap on it the embedding host
//!   passes in ([`listen::Listen`]): Reference Mode alone, or the stems too.
//! - [`folder`] — a song's files on disk or in memory, read one way.
//! - [`open_core`] — opening a song, the one way: which file, prepared or
//!   not, loaded into an engine; what the app adds comes in as an
//!   [`open_core::OpenHost`].
//! - [`section_colors`] — a song's sections in keyflow's colours.
//! - [`reference`] — a song's reference: what it leaves out and how it is
//!   mixed over the render.
//! - [`reference_play`] (native) — a song heard by its reference: the
//!   audio engine's post-render hook that mixes it in.
//! - [`guide_instrument`] — the click, count and cues, played live.
//! - [`web_audio`] (`web`) — the page's audio loop and its `AudioWorklet`.
//!
//! `session-engine` re-exports each from its old path.

pub mod folder;
pub mod guide_instrument;
pub mod listen;
pub mod open_core;
pub mod reference;
#[cfg(not(target_arch = "wasm32"))]
pub mod reference_play;
pub mod section_colors;
pub mod song_stream;
pub mod source;
#[cfg(feature = "web")]
pub mod web_audio;
