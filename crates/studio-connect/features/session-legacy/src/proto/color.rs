//! Auto-colour — REAPER-facing action contract.
//!
//! The trait only; `crate::color` is the implementation. Same split as
//! the other action contracts in this crate.
//!
//! Colour lives in the session domain because it is about to grow past
//! "name matches a rule, paint it": section-aware and setlist-aware
//! colouring need song structure, which only session has.

#[architect::actions(namespace = "FTS_SESSION")]
pub trait AutoColorActions {
    #[action(
        description = "Apply session auto-color rules to all tracks and keep auto-color enabled",
        category = "Session",
        group = "Auto Color"
    )]
    fn auto_color_color_all(&self);
    #[action(
        description = "Apply session auto-color rules to selected tracks",
        category = "Session",
        group = "Auto Color"
    )]
    fn auto_color_color_selected(&self);
    #[action(
        description = "Toggle session auto-color for all tracks",
        category = "Session",
        group = "Auto Color"
    )]
    fn auto_color_toggle(&self);
    #[action(
        description = "Clear colors from all tracks and disable session auto-color",
        category = "Session",
        group = "Auto Color"
    )]
    fn auto_color_clear_all(&self);
    #[action(
        description = "Clear colors from selected tracks",
        category = "Session",
        group = "Auto Color"
    )]
    fn auto_color_clear_selected(&self);
}
