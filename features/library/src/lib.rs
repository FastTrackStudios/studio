//! Studio's library (docs/design/library.md): one place for everything a
//! person or a band has, organized and opened from the iPad first.
//!
//! - [`model`]: songs (the hub) and their arrangements, setlists of pieces,
//!   rigs attached to a setlist as parts, song presets, albums, sessions,
//!   conversations; and the rules that read across them, such as which
//!   sound a rig plays for an entry ([`model::sound`]).
//! - [`fixture`]: a band's library to design and review the screens
//!   against, until the synced docs feed them.
//! - [`task`]: the library as an account's Task orgs have it.
//! - `ui` (feature `ui`): the library screens, inline-styled for Blitz.

pub mod fixture;
pub mod model;
pub mod task;
#[cfg(feature = "ui")]
pub mod ui;

pub use model::Library;
