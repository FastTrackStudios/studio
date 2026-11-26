//! The dock's model with no UI: the [`dockspace`] (panels, stacks, splits
//! and edge docks as plain data), its [`screenset`]s, and the
//! [`DockIntent`] a dock control asks for.
//!
//! An intent travels as an action's arguments (the engine's
//! `SESSION_PANEL_*` / `SESSION_DOCK_*`), so the engine names it without
//! depending on the dock's UI. `dock` re-exports all of this from its old
//! paths.

pub mod dockspace;
pub mod intent;
pub mod screenset;

pub use intent::DockIntent;
