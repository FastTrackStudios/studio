//! The project actions. They go with `engine-project`.

use architect::action::ActionList;

/// The project domain's actions (engine-project): new projects and
/// instrument tracks, the library, organizing, ingest.
///
/// # Errors
///
/// The first registration the list refuses.
pub(crate) fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    crate::library_actions::register(list)?;
    crate::project_new::register(list)?;
    crate::instrument_tracks::register(list)?;
    crate::organize_edit::register(list)?;
    crate::ingest::actions::register(list)?;
    Ok(())
}
