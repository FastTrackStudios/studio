//! Signal's command core in REAPER: what each `FTS_SIGNAL_*` action does in
//! a project.
//!
//! The extension registers the same ids with REAPER's action list and puts
//! them on an architect `ActionList` ([`list`]) with [`ReaperCommands`]
//! behind them; a trigger runs its id on that list ([`dispatch`]) instead of
//! matching on the command name. Navigation and the variation switches need
//! the `SignalController`, which REAPER does not host yet, so they are
//! listed disabled with [`NEEDS_CONTROLLER`] as the reason (they run on the
//! engine's rig, `guitar::signal_commands`).

use std::future::Future;
use std::sync::Arc;

use architect::action::{ActionError, ActionList};
use daw::rpc::Daw;
use signal::commands::{CommandOutcome, SignalCommand, SignalCommandHost};

use crate::{demo_profile, demo_rig, demo_setlist, macro_learn, place_switch, scene_midi};

/// Why navigation is disabled in REAPER.
pub const NEEDS_CONTROLLER: &str =
    "needs the SignalController, which REAPER does not host yet (the engine's rig runs it)";

/// Why `FTS_SIGNAL_MACRO_ADD` is disabled.
pub const MACRO_ADD_PENDING: &str = "adding a macro knob is not implemented yet";

/// A REAPER project as the host of Signal's commands.
#[derive(Clone)]
pub struct ReaperCommands {
    daw: Daw,
}

impl ReaperCommands {
    /// Commands run against `daw`.
    #[must_use]
    pub const fn new(daw: Daw) -> Self {
        Self { daw }
    }
}

impl SignalCommandHost for ReaperCommands {
    fn availability(&self, command: SignalCommand) -> Result<(), String> {
        availability_in_reaper(command)
    }

    fn run(
        &self,
        command: SignalCommand,
    ) -> impl Future<Output = Result<CommandOutcome, String>> + Send + 'static + use<> {
        let daw = self.daw.clone();
        async move { run_in_reaper(&daw, command).await }
    }
}

/// Whether REAPER can run `command`, and why not.
///
/// # Errors
///
/// Why `command` cannot run in REAPER.
pub fn availability_in_reaper(command: SignalCommand) -> Result<(), String> {
    match command {
        SignalCommand::NextSong
        | SignalCommand::PreviousSong
        | SignalCommand::NextSection
        | SignalCommand::PreviousSection
        | SignalCommand::SwitchToVariation(_) => Err(NEEDS_CONTROLLER.to_owned()),
        SignalCommand::MacroAdd => Err(MACRO_ADD_PENDING.to_owned()),
        _ => Ok(()),
    }
}

async fn run_in_reaper(daw: &Daw, command: SignalCommand) -> Result<CommandOutcome, String> {
    availability_in_reaper(command)?;
    let done = match command {
        SignalCommand::DevLoadDemoGuitarRig => demo_rig::load_demo_guitar_rig(daw).await,
        SignalCommand::DevLoadDemoGuitarProfile => demo_profile::load_demo_profile(daw).await,
        SignalCommand::DevGenerateSceneMidiItems => {
            scene_midi::generate_scene_midi_items(daw).await
        }
        SignalCommand::DevLoadDemoSetlist => demo_setlist::load_demo_setlist(daw).await,
        SignalCommand::PlaceSectionSwitch => place_switch::place_section_switch(daw).await,
        SignalCommand::PlaceSongSwitch => place_switch::place_song_switch(daw).await,
        SignalCommand::PlaceSceneSwitch => place_switch::place_scene_switch(daw).await,
        SignalCommand::MacroArm => macro_learn::handle_macro_arm(daw).await,
        SignalCommand::MacroDisarm => macro_learn::handle_macro_disarm(daw).await,
        SignalCommand::MacroSetPoint => macro_learn::handle_macro_set_point(daw).await,
        SignalCommand::MacroRemoveLastPoint => {
            macro_learn::handle_macro_remove_last_point(daw).await
        }
        SignalCommand::MacroSetMin => macro_learn::handle_macro_set_min(daw).await,
        SignalCommand::MacroSetMax => macro_learn::handle_macro_set_max(daw).await,
        SignalCommand::MacroClear => macro_learn::handle_macro_clear(daw).await,
        // Refused by `availability_in_reaper` above.
        SignalCommand::NextSong
        | SignalCommand::PreviousSong
        | SignalCommand::NextSection
        | SignalCommand::PreviousSection
        | SignalCommand::SwitchToVariation(_) => return Err(NEEDS_CONTROLLER.to_owned()),
        SignalCommand::MacroAdd => return Err(MACRO_ADD_PENDING.to_owned()),
    };
    done.map(|()| CommandOutcome::new(command.meta().display_name))
        .map_err(|e| format!("{e:#}"))
}

/// Signal's commands on a new list, run against `daw`.
///
/// # Errors
///
/// As `signal::commands::list` (not on a fresh list, in practice).
pub fn list(daw: Daw) -> Result<ActionList, ActionError> {
    signal::commands::list(&Arc::new(ReaperCommands::new(daw)))
}

/// Run REAPER named command `command_name` (a triggered `FTS_SIGNAL_*`
/// action) on `list`.
///
/// # Errors
///
/// No such action, it is disabled here, or it failed.
pub async fn dispatch(list: &ActionList, command_name: &str) -> Result<(), ActionError> {
    list.run_json_async(command_name, b"").await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaper_runs_the_project_commands_and_disables_the_rest_with_a_reason() {
        let runs = SignalCommand::all()
            .filter(|c| availability_in_reaper(*c).is_ok())
            .count();
        // Four dev loads, three switch placers, seven macro-learn steps.
        assert_eq!(runs, 14);
        assert_eq!(
            availability_in_reaper(SignalCommand::NextSong),
            Err(NEEDS_CONTROLLER.to_owned())
        );
        assert_eq!(
            availability_in_reaper(SignalCommand::MacroAdd),
            Err(MACRO_ADD_PENDING.to_owned())
        );
    }
}
