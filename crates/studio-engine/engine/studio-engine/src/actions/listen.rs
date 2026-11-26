//! The listening actions. They go with `engine-listen`.

use architect::action::ActionList;

/// The listening domain's actions (engine-listen): listening and
/// measuring through the render worker, Signal's offline blocks, the
/// audio's health.
///
/// # Errors
///
/// The first registration the list refuses.
pub(crate) fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    crate::signal_offline::register(list)?;
    #[cfg(feature = "native")]
    crate::render_worker::register_routed(list, crate::mix_measure::register)?;
    #[cfg(feature = "native")]
    crate::render_worker::register_routed(list, crate::listen::register)?;
    #[cfg(feature = "native")]
    crate::render_worker::register(list)?;
    crate::audio_health::register(list)?;
    Ok(())
}
