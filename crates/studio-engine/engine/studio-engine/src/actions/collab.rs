//! The collaboration actions. They go with `engine-collab`.

use architect::action::ActionList;

/// The collaboration domain's actions (engine-collab): the agent.
///
/// # Errors
///
/// The first registration the list refuses.
pub(crate) fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    crate::agent::register(list)?;
    Ok(())
}
