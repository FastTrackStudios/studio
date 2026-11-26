//! The mix's actions here: the bass rig. They go with `engine-mix`.

#[allow(
    clippy::wildcard_imports,
    reason = "split out of actions.rs: the parent's helpers and imports, as before"
)]
use super::*;

#[cfg(feature = "signal")]
pub(crate) static BASS_RIG: ActionMeta = meta(
    "BASS_RIG",
    "bass_rig",
    "Bass rig on the bass",
    "signal's bass rig (FTS Bass Rig) on each track under Bass, first in its chain: a signal bass `profile` (`Metal` unless given; from Session's own copy of signal's bass library, never the live rig's) with every scene loaded — Clean, Crunch, Drive … — starting on the scene its name holds, else the profile's default. Its Scene parameter switches them and is automated like any other (`FTS Bass Rig/Scene`, by name: `\"Crunch\"`). `patches`: true gives the older fixed patches instead (Clean: the DP3X fed low; Harmonics: the DP3X as captured; Crunch: an HM2 into it). Reran, the rig there takes it again. `dry_run`.",
    "Bass",
    "",
);

/// Whether to only say.
#[cfg(feature = "signal")]
#[derive(Facet)]
pub(crate) struct BassRigArg {
    profile: Option<String>,
    patches: Option<bool>,
    dry_run: Option<bool>,
}

#[cfg(feature = "signal")]
pub(crate) fn register_bass_rig(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&BASS_RIG, Effect::Mutating, |a: BassRigArg| {
        let (daw, project) = song()?;
        let setup = if a.patches.unwrap_or(false) {
            crate::bass_rig::Setup::Patches
        } else {
            crate::bass_rig::Setup::Profile(a.profile.unwrap_or_else(|| "Metal".to_owned()))
        };
        crate::bass_rig::set_up(daw, &project, &setup, a.dry_run.unwrap_or(false))
    })?;
    Ok(())
}

/// The mix domain's actions (engine-mix): the FX, sends, the mix's state,
/// snapshots and variants, automation, the Signal kit and rigs.
///
/// # Errors
///
/// The first registration the list refuses.
pub(crate) fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    #[cfg(feature = "signal")]
    register_bass_rig(list)?;
    #[cfg(feature = "native")]
    crate::fx_actions::register(list)?;
    #[cfg(feature = "native")]
    crate::signal_kit::register(list)?;
    crate::clap_host::signal::register(list)?;
    #[cfg(feature = "native")]
    crate::mix_sends::register(list)?;
    #[cfg(feature = "native")]
    crate::mix_state::register(list)?;
    #[cfg(feature = "native")]
    crate::mix_snapshot::register(list)?;
    #[cfg(feature = "native")]
    crate::mix_variants::register(list)?;
    crate::automation_actions::register(list)?;
    #[cfg(feature = "signal")]
    crate::rig_actions::register(list)?;
    Ok(())
}
