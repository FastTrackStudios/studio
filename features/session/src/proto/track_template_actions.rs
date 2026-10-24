//! Track-template creation under the `FTS_SESSION` namespace.
//!
//! These are a *second* name for actions `dynamic-template` already
//! registers as `FTS_DYNAMIC_TEMPLATE_*`. They exist because committed FTS
//! config still binds the old names — `reaper-input`'s `tracks.styx` /
//! `mode-organize.styx` keybindings and `fts-icons`' `tracks.toml` toolbar
//! assignments — so retiring them means repointing those files *and*
//! re-running `fts-icons build --install`. That is a sequenced change, not
//! a refactor.
//!
//! `crate::daw_module` registers these and forwards each trigger to
//! `dynamic_template::daw_module::dispatch_session_command`, which maps the
//! alias onto the real id (including the two that were renamed:
//! `ELECTRONIC_DRUMS` → `ELECTRONIC_KIT`, `SYNTH` → `BASS_SYNTH`).
//!
//! Namespace is `FTS_SESSION` so generated ids match the REAPER
//! named-command convention already in use.

#[architect::actions(namespace = "FTS_SESSION")]
pub trait TrackTemplateActions {
    #[action(
        description = "Organize all project tracks using the dynamic template track hierarchy",
        category = "Session",
        group = "Tracks"
    )]
    fn organize_everything(&self);

    #[action(
        description = "Organize selected tracks using the dynamic template track hierarchy",
        category = "Session",
        group = "Tracks"
    )]
    fn organize_selected_tracks(&self);

    #[action(
        description = "Create a new drum kit track group",
        category = "Session",
        group = "Tracks"
    )]
    fn create_new_drum_kit(&self);

    #[action(
        description = "Create a new electronic drums track group",
        category = "Session",
        group = "Tracks"
    )]
    fn create_new_electronic_drums(&self);

    #[action(
        description = "Create a new bass guitar track group",
        category = "Session",
        group = "Tracks"
    )]
    fn create_new_bass_guitar(&self);

    #[action(
        description = "Create a new electric guitar track group",
        category = "Session",
        group = "Tracks"
    )]
    fn create_new_electric_guitar(&self);

    #[action(
        description = "Create a new acoustic guitar track group",
        category = "Session",
        group = "Tracks"
    )]
    fn create_new_acoustic_guitar(&self);

    #[action(
        description = "Create a new keys track group (piano, organ, electric keys, etc.)",
        category = "Session",
        group = "Tracks"
    )]
    fn create_new_keys(&self);

    #[action(
        description = "Create a new synth track group (bass, lead, pad, arp, etc.)",
        category = "Session",
        group = "Tracks"
    )]
    fn create_new_synth(&self);

    #[action(
        description = "Create a new lead vocals track group",
        category = "Session",
        group = "Tracks"
    )]
    fn create_new_lead_vocals(&self);

    #[action(
        description = "Create a new background vocals track group",
        category = "Session",
        group = "Tracks"
    )]
    fn create_new_background_vocals(&self);

    #[action(
        description = "Create a new orchestral brass track group",
        category = "Session",
        group = "Tracks"
    )]
    fn create_new_orchestral_brass(&self);

    #[action(
        description = "Create a new orchestral woodwinds track group",
        category = "Session",
        group = "Tracks"
    )]
    fn create_new_orchestral_woodwinds(&self);

    #[action(
        description = "Create a new orchestral strings track group",
        category = "Session",
        group = "Tracks"
    )]
    fn create_new_orchestral_strings(&self);

    #[action(
        description = "Create a new orchestral percussion track group",
        category = "Session",
        group = "Tracks"
    )]
    fn create_new_orchestral_percussion(&self);

    #[action(
        display_name = "Create New SFX",
        description = "Create a new SFX track group",
        category = "Session",
        group = "Tracks"
    )]
    fn create_new_sfx(&self);
}
