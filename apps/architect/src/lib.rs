//! Studio's distribution of Architect (`docs/architect-distribution.md`).
//!
//! The Architect app (`architect_desktop::distribution::architect()`: its
//! first-party extensions, the store as a registry, installs at run time)
//! with Studio's extensions **compiled in**, under an identity of its own:
//!
//! | | Studio Architect | the Architect app | Studio's own app |
//! | --- | --- | --- | --- |
//! | name | `Studio Architect` | `Architect` | `Studio` |
//! | binary / app | `studio-architect` | `architect-desktop` | `studio-desktop` |
//! | config folder | `studio-architect` | `architect` | (Studio's prefs) |
//! | bundle id | `app.fasttrackstudio.studio-architect` | `app.fasttrackstudio.architect` | `app.fasttrackstudio.studio` |
//!
//! Compiled in, as the `studio` bundle: Setlists ([`setlists::ext::Setlists`])
//! and Productions ([`productions::ext::Productions`]); and Studio's block
//! renderer ([`setlists::share::blocks`]): a song's chart fence drawn as
//! Keyflow's follow SVG and a set's fence as its playable list, on the
//! node's share links and Sites. That renderer is the reason this exists:
//! an installed (store) process extension cannot draw on a node
//! (FastTrackStudios/architect#547), so a stock node shows a chart's
//! Keyflow text where this one draws the chart.
//!
//! An installed copy of either extension still overrides the compiled-in
//! one, as everywhere in Architect.

use architect_desktop::{Bundle, Distribution, Icon};

/// The distribution's name.
pub const NAME: &str = "Studio Architect";

/// Its bundle id: distinct from the Architect app's and Studio's own.
pub const BUNDLE_ID: &str = "app.fasttrackstudio.studio-architect";

/// Its app name and its folder under the platform's config dir.
pub const FOLDER: &str = "studio-architect";

/// Its icon: Architect's columns over Studio's staff.
pub const ICON: &str = include_str!("../icon.svg");

/// Studio's extensions, compiled in together: a production's show set is
/// a setlist (`productions` writes the notes `setlists` reads).
#[must_use]
pub fn studio() -> Bundle {
    Bundle::new("studio", "Studio")
        .member(&setlists::ext::Setlists)
        .member(&productions::ext::Productions)
        .member(&studio_views::ext::StudioViews)
}

/// Studio Architect: the Architect app, renamed, with [`studio`] compiled
/// in and Studio's blocks drawn on its share links and Sites.
#[must_use]
pub fn distribution() -> Distribution {
    architect_desktop::distribution::architect()
        .named(NAME)
        .app(FOLDER)
        .folder(FOLDER)
        .bundle_id(BUNDLE_ID)
        .version(env!("CARGO_PKG_VERSION"))
        .icon(Icon::svg(ICON).tray("music.note.list", "audio-x-generic"))
        .bundle(studio())
        .block(setlists::share::blocks())
}
