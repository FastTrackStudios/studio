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

pub mod island;

/// Where Studio's island ships: `studio-island/` beside the binary (the
/// release puts `just island`'s folder there: scripts/studio-architect.sh).
pub const ISLAND_FOLDER: &str = "studio-island";

/// Studio's island (#445), from the folder beside the binary, if it matches its lock.
///
/// Session's progress bar and transport on a share link's now-playing
/// view, a WASI component (`apps/architect/island`) the node serves from
/// its own `/_ext/` (`architect_sites::island`). Read
/// from [`ISLAND_FOLDER`] beside the binary (part of the distribution as
/// shipped, never a vault's or the node's config) and checked against its
/// lock ([`island::at`]). None when it is not there, or when it does not
/// match its lock (one warning): the view keeps Architect's own bar and
/// controls.
#[must_use]
pub fn island() -> Option<architect_sites::SiteIsland> {
    let dir = std::env::current_exe().ok()?.parent()?.join(ISLAND_FOLDER);
    match island::at(&dir) {
        Ok(island) => Some(island),
        Err(island::IslandError::Absent(_)) => None,
        Err(error) => {
            tracing::warn!(
                island.outcome = "refused",
                island.reason = %error,
                "studio-architect: its island does not match its lock; serving none"
            );
            None
        }
    }
}

/// Studio Architect: the Architect app, renamed, with [`studio`] compiled
/// in and Studio's blocks drawn on its share links and Sites, and its
/// island ([`island`]) when it ships with it.
#[must_use]
pub fn distribution() -> Distribution {
    distribution_with(island())
}

/// [`distribution`] with `island` (or none) for its island.
#[must_use]
pub fn distribution_with(island: Option<architect_sites::SiteIsland>) -> Distribution {
    let dist = base();
    match island {
        Some(island) => dist.island(island),
        None => dist,
    }
}

fn base() -> Distribution {
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
