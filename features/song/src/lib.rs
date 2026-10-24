//! `song` — the durable **Song + Arrangement** schema.
//!
//! A [`Song`] is a portable, self-contained vault folder (plaintext +
//! attachment references) — the *stored* form of a song. It is NOT the
//! runtime `session_proto::Song` (that is hydrated from this later, out of
//! scope here).
//!
//! A song has an identity plus one or more [`Arrangement`]s: there is a
//! **default arrangement**, and more can be added over time. Each
//! arrangement carries its [`Key`], a chart **reference** ([`ChartRef`]),
//! its parts ([`PartsManifest`]), and attachment references
//! ([`AttachmentRef`]).
//!
//! ## Keyflow-agnostic by design
//!
//! This crate has **no dependency on `keyflow`**. Charts are stored as
//! references — a relative path within the song folder and/or an
//! attachment id — never as an embedded `keyflow::Chart`. Parsing and
//! rendering charts is a separate, later workstream.
//!
//! ## On-disk form
//!
//! [`to_folder`] / [`from_folder`] round-trip a [`Song`] to a folder. See
//! `docs/song-folder-format.md` for the layout.

#[cfg(feature = "attachments")]
pub mod attachments;
pub mod folder;
pub mod key;
pub mod model;

pub use folder::{ReadError, WriteError, from_folder, to_folder};
pub use key::{Accidental, Key, Letter, Mode, ParseKeyError};
pub use model::{
    Arrangement, ArrangementId, AttachmentRef, ChartRef, Part, PartsManifest, ResourceRef, Song,
    SongId, TimeSignature,
};
