//! Setlist build / demo / diagnostics — REAPER-facing action contract.
//!
//! The trait only; `crate::setlist::actions::SetlistActionsImpl<D>` is
//! the implementation. Same split as `track_manager` / `playback` /
//! `mode`.
//!
//! Namespace is `FTS_SESSION` (not the macro's trait-derived default
//! `SETLIST`) so generated ids match the REAPER named-command
//! convention already in use — `FTS_SESSION_BUILD_SETLIST` etc.

#[architect::actions(namespace = "FTS_SESSION")]
pub trait SetlistActions {
    #[action(
        description = "Scan every open REAPER project tab, parse SONGSTART/SONGEND markers and section regions, and rebuild the cached Setlist",
        category = "Setlist",
        group = "Build"
    )]
    fn build_setlist(&self);

    #[action(
        description = "Stamp a 3-song demo setlist (markers + section regions) into the current project, then rebuild the cached Setlist",
        category = "Setlist",
        group = "Demo"
    )]
    fn load_demo_setlist(&self);

    #[action(
        description = "Log every marker and region in the current project with position, name, color and ruler lane index — diagnostic for lane-assignment issues",
        category = "Debug",
        group = "Diagnostics"
    )]
    fn dump_ruler_state(&self);
}
