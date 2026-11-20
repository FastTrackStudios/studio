//! Studio's distribution and its compiled-in extensions, listed once.
//!
//! The desktop app (`studio`, apps/desktop), the web client (`studio-web`,
//! apps/web) and the iPhone and iPad app (`studio-mobile`, apps/mobile) are
//! the same distribution of Architect: [`distribution`] (its name, bundle
//! id, folder, icon, Studio's extensions and, with `stage`, the window's
//! views and the launcher as the home). Each app adds only what its
//! platform has (the desktop's island, share-link blocks and shipped
//! engine; the browser's linked island views) and hands it to its
//! platform's `run`. Add an extension here and every app has it.
//!
//! Not yet in the browser build (each has a way there, none is left out
//! by design): Studio's engine (`daw`: natively a process, in a browser
//! session-engine's `web` engine in the page, the engine-through-a-link
//! work) and the rig views. Ignition's lighting builds for the browser
//! (its room drawn by Ignition's web renderer), and studio-web links it
//! itself until `stage` does.

use architect_distribution::{Bundle, Distribution, Icon};

/// The distribution's name.
pub const NAME: &str = "Studio";

/// Its bundle id (the App Store's and TestFlight's too).
pub const BUNDLE_ID: &str = "app.fasttrackstudio.studio";

/// Its app name and its folder under the platform's config dir.
pub const FOLDER: &str = "studio";

/// Its icon: Architect's columns over Studio's staff.
pub const ICON: &str = include_str!("../icon.svg");

/// The view a signed-in person's app opens on: the launcher's Home.
pub const HOME: (&str, &str) = ("studio.launcher", "launcher");

/// Studio, on every platform: the Architect app renamed, with Studio's
/// extensions and, with `stage`, the window's views, opening on the
/// launcher.
#[must_use]
pub fn distribution() -> Distribution {
    let dist = architect_distribution::architect()
        .named(NAME)
        .app(FOLDER)
        .folder(FOLDER)
        .bundle_id(BUNDLE_ID)
        .version(env!("CARGO_PKG_VERSION"))
        .icon(Icon::svg(ICON).tray("music.note.list", "audio-x-generic"))
        .bundle(studio());
    #[cfg(feature = "stage")]
    let dist = dist.bundle(stage()).home(HOME.0, HOME.1);
    dist
}

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
