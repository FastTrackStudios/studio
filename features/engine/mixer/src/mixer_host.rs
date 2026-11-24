//! The mixer installed into the window: what it hands the views that show
//! a mixer, a close-up or the FX view, and its view actions
//! (`crate::dock::MixerViews`). Call [`install`] once at start.

use dioxus::prelude::*;

/// Install the mixer's views and actions for every window of this process.
pub fn install() {
    crate::dock::install_mixer(crate::dock::MixerViews {
        mixer: group_mixer,
        #[cfg(feature = "web")]
        web_daw_panels: |engine, docked, mixer_only| {
            rsx! { crate::mixer_panel::WebDawPanels { engine, docked, mixer_only } }
        },
        use_closeups: || {
            use_context_provider(crate::closeup::Closeups::new);
        },
        closeup_layer: |landscape| rsx! { crate::closeup::CloseupLayer { landscape } },
        fx_panel: || rsx! { crate::fx_view::FxPanel {} },
        register: crate::mixer_actions::register,
    });
}

/// A mixer on its own, its links from the context.
fn group_mixer() -> Element {
    #[cfg(feature = "native")]
    return rsx! { crate::mixer_panel::Mixer {} };
    #[cfg(all(feature = "web", not(feature = "native")))]
    return rsx! { crate::mixer_panel::WebMixer { hidden: false } };
    #[cfg(not(any(feature = "native", feature = "web")))]
    rsx! {}
}
