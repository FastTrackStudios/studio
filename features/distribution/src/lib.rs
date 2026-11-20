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
#[cfg(all(feature = "stage", not(target_arch = "wasm32")))]
#[must_use]
pub fn stage() -> Bundle {
    Bundle::new("stage", "Stage")
        .member(&studio_launcher::Launcher)
        .member(&keyflow_extension::Keyflow)
        .member(&studio_arrangement::Arrangement)
        .member(&studio_mixer::Mixer)
        .member(&studio_lyrics::Lyrics)
        .member(&studio_navigator::Navigator)
        .member(&ignition_pane::ext::Lighting)
}

/// The window's extensions in a browser: the launcher (the home) in the
/// page's own module, and every other one deferred, a wasm chunk of its
/// own fetched the first time one of its views opens or one of its
/// actions runs ([`deferred`]): opening the arrangement fetches the
/// arrangement, not the mixer, Keyflow's chart or Ignition's lighting.
/// Studio's engine (`daw`) is the page's own, as the launcher is.
#[cfg(all(feature = "stage", target_arch = "wasm32"))]
#[must_use]
pub fn stage() -> Bundle {
    use architect_extensions::linked::deferred::Deferred;
    // Studio's engine is not deferred: its build opens a share link's set
    // as the page loads, and the arrangement and the mixer read the
    // engine itself, not through its actions.
    Bundle::new("stage", "Stage")
        .member(&daw_extension::Daw)
        .member(&studio_launcher::Launcher)
        .member(&Deferred::<deferred::Keyflow>::NEW)
        .member(&Deferred::<deferred::Arrangement>::NEW)
        .member(&Deferred::<deferred::Mixer>::NEW)
        .member(&Deferred::<deferred::Lyrics>::NEW)
        .member(&Deferred::<deferred::Navigator>::NEW)
        .member(&Deferred::<deferred::Lighting>::NEW)
}

/// The browser build's deferred extensions: each one's manifest here, its
/// code (`Extension::build` and everything only it reaches) in a wasm
/// chunk of its own (`#[wasm_split]`, feature `wasm-split`), split out by
/// `dx build --wasm-split`. Without the feature (`dx serve`) the code is
/// the page's module and only the building waits for first use.
#[cfg(all(feature = "stage", target_arch = "wasm32"))]
pub mod deferred {
    use architect_extensions::linked::deferred::{Defer, Loading};
    use architect_extensions::linked::{Builder, Extension as _};
    #[cfg(feature = "wasm-split")]
    use wasm_splitter as wasm_split;

    /// `$name`: extension `$ext`, its code in chunk `$chunk`.
    macro_rules! deferred {
        ($name:ident, $chunk:ident, $ext:expr) => {
            #[doc = concat!("`", stringify!($ext), "`, its code in the `", stringify!($chunk), "` chunk.")]
            pub struct $name;

            impl Defer for $name {
                fn manifest() -> &'static str {
                    $ext.manifest()
                }

                fn files() -> &'static [(&'static str, &'static str)] {
                    $ext.files()
                }

                fn load() -> Loading {
                    Box::pin($chunk())
                }
            }

            #[cfg_attr(feature = "wasm-split", wasm_split::wasm_split($chunk))]
            async fn $chunk() -> Builder {
                Builder::of(&$ext)
            }
        };
    }

    deferred!(Keyflow, chart, keyflow_extension::Keyflow);
    deferred!(Arrangement, arrangement, studio_arrangement::Arrangement);
    deferred!(Mixer, mixer, studio_mixer::Mixer);
    deferred!(Lyrics, lyrics, studio_lyrics::Lyrics);
    deferred!(Navigator, navigator, studio_navigator::Navigator);
    deferred!(Lighting, lighting, ignition_pane::ext::Lighting);
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
