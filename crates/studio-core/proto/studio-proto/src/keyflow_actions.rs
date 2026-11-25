//! Keyflow section/marker insertion — REAPER-facing action contract.
//!
//! The trait only; `crate::keyflow::actions::KeyflowActionsImpl<D>` is
//! the implementation. Same split as `track_manager` / `playback` /
//! `mode` / `setlist_actions`.
//!
//! Namespace `FTS_SESSION` matches the REAPER command-id convention
//! already in use (`FTS_SESSION_INSERT_INTRO_REGION`, …).

#[architect::actions(namespace = "FTS_SESSION")]
pub trait KeyflowActions {
    #[action(
        description = "Insert an Intro section region at the current edit cursor",
        category = "Session",
        group = "Edit"
    )]
    fn insert_intro_region(&self);
    #[action(
        description = "Insert a Verse section region at the current edit cursor",
        category = "Session",
        group = "Edit"
    )]
    fn insert_verse_region(&self);
    #[action(
        description = "Insert a Pre-Chorus section region at the current edit cursor",
        category = "Session",
        group = "Edit"
    )]
    fn insert_pre_chorus_region(&self);
    #[action(
        description = "Insert a Chorus section region at the current edit cursor",
        category = "Session",
        group = "Edit"
    )]
    fn insert_chorus_region(&self);
    #[action(
        description = "Insert a Bridge section region at the current edit cursor",
        category = "Session",
        group = "Edit"
    )]
    fn insert_bridge_region(&self);
    #[action(
        description = "Insert an Outro section region at the current edit cursor",
        category = "Session",
        group = "Edit"
    )]
    fn insert_outro_region(&self);
    #[action(
        description = "Insert an Instrumental section region at the current edit cursor",
        category = "Session",
        group = "Edit"
    )]
    fn insert_instrumental_region(&self);
    #[action(
        description = "Insert a Solo section region at the current edit cursor",
        category = "Session",
        group = "Edit"
    )]
    fn insert_solo_region(&self);
    #[action(
        description = "Insert a Hits section region at the current edit cursor",
        category = "Session",
        group = "Edit"
    )]
    fn insert_hits_region(&self);
    #[action(
        description = "Insert an Interlude section region at the current edit cursor",
        category = "Session",
        group = "Edit"
    )]
    fn insert_interlude_region(&self);
    #[action(
        description = "Insert a Breakdown section region at the current edit cursor",
        category = "Session",
        group = "Edit"
    )]
    fn insert_breakdown_region(&self);
    #[action(
        description = "Insert a Vamp section region at the current edit cursor",
        category = "Session",
        group = "Edit"
    )]
    fn insert_vamp_region(&self);
    #[action(
        description = "Insert a Count-In section region at the current edit cursor",
        category = "Session",
        group = "Edit"
    )]
    fn insert_count_in_region(&self);
    #[action(
        description = "Insert an End section region at the current edit cursor",
        category = "Session",
        group = "Edit"
    )]
    fn insert_end_region(&self);
    #[action(
        description = "Insert a Count-In marker on the MARKS ruler lane",
        category = "Session",
        group = "Edit"
    )]
    fn insert_count_in_marker(&self);
    #[action(
        description = "Insert an =START marker on the MARKS ruler lane",
        category = "Session",
        group = "Edit"
    )]
    fn insert_start_marker(&self);
    #[action(
        description = "Insert an =END marker on the MARKS ruler lane",
        category = "Session",
        group = "Edit"
    )]
    fn insert_end_marker(&self);
    #[action(
        description = "Insert a SONGSTART marker on the MARKS ruler lane",
        category = "Session",
        group = "Edit"
    )]
    fn insert_songstart_marker(&self);
    #[action(
        description = "Insert a SONGEND marker on the MARKS ruler lane",
        category = "Session",
        group = "Edit"
    )]
    fn insert_songend_marker(&self);
    #[action(
        description = "Convert plain section-name markers into FTS section regions and add a SONG-lane region named after the project",
        category = "Session",
        group = "Edit"
    )]
    fn convert_markers_to_session_format(&self);
}
