//! Editing a recorded performance: its timing (quantize) and its
//! dynamics (level) — one engine, with what is particular to a material in a
//! profile. Drums are the first profile.
//!
//! Design and migration: `docs/design/editing.md`. Spec:
//! `docs/spec/session/drum-editing.md`.
//!
//! - [`quantize`]: which line a played event goes to, what moves together,
//!   how a run keeps its order, and how a slip moves what is in it.
//! - [`level`]: leveling a stretch of a part, the curve kept outside it.
//! - [`groove`]: a section's groove learned from its own bars.
//! - [`written`]: a played part matched to the part as written.
//! - [`settings`]: a quantize's settings and a call's patch over them.
//! - [`scope`]: what is selected, and the profile it is edited with.
//! - [`drums`]: the kit's profile — pieces, the grid scan, fills, moments.
//!
//! Nothing here reads or writes a project: times are seconds (or beats,
//! for the groove), and the caller brings the grid (`line_of`).

#![forbid(unsafe_code)]

pub mod drums;
pub mod groove;
pub mod level;
pub mod quantize;
pub mod scope;
pub mod settings;
pub mod written;
