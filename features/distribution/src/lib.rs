//! Studio's compiled-in extensions, listed once.
//!
//! The desktop app (`studio`, apps/desktop) and the web client
//! (`studio-web`, apps/web) are the same distribution of Architect
//! and link the same extensions: [`studio`], [`stage`] and [`rig`]. Add an
//! extension here and both apps have it.
//!
//! Not yet in the browser build (each has a way there, none is left out
//! by design): Studio's engine (`daw`: natively a process, in a browser
//! session-engine's `web` engine in the page, the engine-through-a-link
//! work) and the rig views. Ignition's lighting builds for the browser
//! (its room drawn by Ignition's web renderer), and studio-web links it
//! itself until `stage` does.

use architect_distribution::Bundle;

/// Studio's extensions: a production's show set is a setlist
/// (`productions` writes the notes `setlists` reads), and Studio's views
/// with the toolbar that opens them.
#[must_use]
pub fn studio() -> Bundle {
    Bundle::new("studio", "Studio")
        .member(&setlists::ext::Setlists)
        .member(&productions::ext::Productions)
        .member(&studio_views::ext::StudioViews)
}

/// The window's extensions, on every target: Studio's arrangement, mixer,
/// lyrics, navigator and launcher, Keyflow's chart and Ignition's lighting.
/// Natively the arrangement and mixer are windows on Studio's engine (the
/// `daw` extension, a process) and Ignition's room a GPU process view; in a
/// browser the stage begins with the engine itself (`daw`, linked: its
/// engine in the page, which the views require through `fts.session`) and
/// the room is Ignition's web renderer in a canvas. A development copy of
/// any of them shadows the linked one, so one can be worked on while the
/// rest run as built.
#[cfg(feature = "stage")]
#[must_use]
pub fn stage() -> Bundle {
    let stage = Bundle::new("stage", "Stage");
    #[cfg(target_arch = "wasm32")]
    let stage = stage.member(&daw_extension::Daw);
    stage
        .member(&studio_launcher::Launcher)
        .member(&keyflow_extension::Keyflow)
        .member(&studio_arrangement::Arrangement)
        .member(&studio_mixer::Mixer)
        .member(&studio_lyrics::Lyrics)
        .member(&studio_navigator::Navigator)
        .member(&ignition_pane::ext::Lighting)
}

/// Signal's rig views (natively, until their browser builds land): the footswitch scenes, the sidebar, the
/// browser, the DSP grid with its FX row and Edit, the macros, the rig's
/// status, the keys rig's screens and the audio settings. Each is a vox
/// client of the rig engine (`studio.rig`, a process of its own as `daw`
/// is).
#[cfg(all(feature = "rig", not(target_arch = "wasm32")))]
#[must_use]
pub fn rig() -> Bundle {
    Bundle::new("rig", "Rig")
        .member(&studio_rig_status::Status)
        .member(&studio_rig_switches::Switches)
        .member(&studio_rig_setlist::Setlist)
        .member(&studio_rig_browser::Browser)
        .member(&studio_rig_grid::Grid)
        .member(&studio_rig_macros::Macros)
        .member(&studio_audio_settings::AudioSettings)
        .member(&studio_keys::Keys)
}
