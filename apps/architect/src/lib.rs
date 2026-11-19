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
    let bundle = Bundle::new("studio", "Studio")
        .member(&setlists::ext::Setlists)
        .member(&studio_chart::ext::Chart)
        .member(&studio_lyrics::ext::Lyrics)
        .member(&productions::ext::Productions)
        .member(&studio_views::ext::StudioViews);
    // The Lighting pane: the show in 3D, a process view (feature `lighting`).
    #[cfg(all(feature = "lighting", target_os = "linux"))]
    let bundle = bundle.member(&ignition_pane::ext::Lighting);
    // The DAW view: a setlist or a song entered into Session's shell
    // (feature `daw`, docs/design/studio-in-architect.md).
    #[cfg(feature = "daw")]
    let bundle = bundle.member(&studio_daw::ext::Daw);
    bundle
}

/// Studio's compiled-in extensions, for what reads their manifests.
fn members() -> Vec<&'static dyn architect_extensions::linked::Extension> {
    let members: Vec<&'static dyn architect_extensions::linked::Extension> = vec![
        &setlists::ext::Setlists,
        &productions::ext::Productions,
        &studio_views::ext::StudioViews,
    ];
    #[cfg(all(feature = "lighting", target_os = "linux"))]
    let members = {
        let mut members = members;
        members.push(&ignition_pane::ext::Lighting);
        members
    };
    #[cfg(feature = "daw")]
    let members = {
        let mut members = members;
        members.push(&studio_daw::ext::Daw);
        members
    };
    members
}

/// The folder of Studio's checkout that has `path`: `STUDIO_CHECKOUT`, else
/// the working directory or one above it.
fn checkout_with(path: &str) -> Option<std::path::PathBuf> {
    let has = |root: &std::path::Path| {
        root.join("Cargo.toml").is_file() && root.join(path).join("extension.toml").is_file()
    };
    if let Some(root) = std::env::var_os("STUDIO_CHECKOUT").map(std::path::PathBuf::from) {
        return has(&root).then_some(root);
    }
    let cwd = std::env::current_dir().ok()?;
    cwd.ancestors().find(|dir| has(dir)).map(std::path::Path::to_path_buf)
}

/// `studio-architect dev-source <id>`: built-in `id`'s crate, written from
/// this checkout (by its `[source]`) into the app's development folder as
/// `EXT_CLONE` writes one — a workspace of its own — unless it is there
/// already (edits in it are kept). Then `EXT_CLONE {id}` builds it and runs
/// it over the built-in, under the same id. Its folder.
///
/// # Errors
///
/// No built-in `id`, or it has no `[source]`, or no checkout of it is
/// found, or the crate cannot be written.
pub fn dev_source(id: &str) -> Result<std::path::PathBuf, String> {
    let manifest = members()
        .into_iter()
        .filter_map(|e| architect_extensions::manifest::Manifest::parse(e.manifest()).ok())
        .find(|m| m.id == id)
        .ok_or_else(|| format!("Studio has no built-in extension {id}"))?;
    let source = manifest
        .source
        .ok_or_else(|| format!("{id} says nowhere its source is ([source])"))?;
    let checkout = checkout_with(&source.path).ok_or_else(|| {
        format!(
            "no checkout with {} here: run it from Studio's checkout, or set STUDIO_CHECKOUT",
            source.path
        )
    })?;
    let folder = distribution()
        .app_dir()
        .ok_or("this app has no folder")?
        .join("dev")
        .join(id);
    if !folder.join("Cargo.toml").is_file() {
        architect_extensions::source::write_crate(&checkout, &source, &folder)?;
    }
    Ok(folder)
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
    // `STUDIO_ISLAND_DIR`: the folder named (a dev run under `dx serve`,
    // whose executable has no `studio-island/` beside it).
    let dir = match std::env::var_os("STUDIO_ISLAND_DIR").filter(|d| !d.is_empty()) {
        Some(dir) => std::path::PathBuf::from(dir),
        None => std::env::current_exe().ok()?.parent()?.join(ISLAND_FOLDER),
    };
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

/// Its bundle id, `.{instance}` after it when `STUDIO_INSTANCE` names
/// one: a dev Studio Desktop beside the person's own is another instance
/// (its own single-instance lock), not handed off to theirs.
fn bundle_id() -> String {
    match std::env::var("STUDIO_INSTANCE") {
        Ok(instance) if !instance.trim().is_empty() => format!("{BUNDLE_ID}.{}", instance.trim()),
        _ => BUNDLE_ID.to_owned(),
    }
}

fn base() -> Distribution {
    architect_desktop::distribution::architect()
        .named(NAME)
        .app(FOLDER)
        .folder(FOLDER)
        .bundle_id(bundle_id())
        .version(env!("CARGO_PKG_VERSION"))
        .icon(Icon::svg(ICON).tray("music.note.list", "audio-x-generic"))
        .bundle(studio())
        .block(setlists::share::blocks())
}

/// The distribution's node in this process, on a thread of its own beside
/// the window (`STUDIO_SERVE=1`): what `studio-architect serve` runs, with
/// the same configuration (`ARCHITECT_CONFIG`, `ARCHITECT_*`), the
/// distribution's blocks, share layers, islands and favourites. Studio
/// Desktop is then the server Studio Web talks to. A node that cannot
/// start says why once and the window opens without it.
pub fn serve_beside() {
    use architect_desktop::node;
    let file = std::env::var_os("ARCHITECT_CONFIG").map(std::path::PathBuf::from);
    let config = match node::config::Config::load(file.as_deref(), |var| std::env::var(var).ok()) {
        Ok(config) => config,
        Err(error) => {
            tracing::warn!(serve.outcome = "no_config", %error, "studio-architect: the node did not start");
            return;
        }
    };
    let spawned = std::thread::Builder::new()
        .name("studio-node".into())
        .spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_multi_thread().enable_all().build() else {
                tracing::warn!(serve.outcome = "no_runtime", "studio-architect: the node did not start");
                return;
            };
            let dist = distribution();
            let blocks = dist.blocks(&config.data_dir.join("linked"));
            let (layers, islands, favourites) = (dist.share_layers(), dist.islands(), dist.favourites());
            runtime.block_on(async move {
                match node::start_with_shell(config, blocks, layers, islands, favourites).await {
                    Ok(running) => {
                        tracing::info!(serve.outcome = "started", serve.http = %running.http, "studio-architect: serving beside the window");
                        // Until the process ends: the window's quit ends it.
                        std::future::pending::<()>().await;
                        drop(running);
                    }
                    Err(error) => {
                        tracing::warn!(serve.outcome = "refused", %error, "studio-architect: the node did not start");
                    }
                }
            });
        });
    if let Err(error) = spawned {
        tracing::warn!(serve.outcome = "no_thread", %error, "studio-architect: the node did not start");
    }
}
