//! Opening a song: THE one way, on every target — now
//! `session_runtime::open_core`, re-exported here.
//!
//! What stays is the engine's side of it, the runtime's
//! [`OpenHost`]: [`Prepare`](crate::prepare::Prepare) prepares a song
//! (organize, chart, guide) and knows the project-file format
//! (`project_file`, `session_file`), which the runtime does not. Every
//! function that took a `&Prepare` takes it still (a `&Prepare` is a
//! `&dyn OpenHost`); the plain loads, which took none, keep their old
//! signatures here and open project files as before.

use std::path::Path;

use daw::standalone::Standalone;

pub use session_runtime::open_core::*;

use crate::folder::Folder;
use crate::prepare::Prepare;

impl OpenHost for Prepare {
    fn nothing_to_prepare(&self) -> bool {
        self.is_empty()
    }

    fn prepare(&self, folder: &dyn Folder, opened: &Opened) -> eyre::Result<()> {
        self.run_in(folder, opened)
    }

    #[cfg(feature = "native")]
    fn is_project_file(&self, path: &Path) -> bool {
        crate::project_file::file_and_branch(path).is_some()
    }

    #[cfg(feature = "native")]
    fn open_project_file(
        &self,
        daw: &Standalone,
        path: &Path,
        media: Media,
    ) -> Option<eyre::Result<Opened>> {
        let (file, branch) = crate::project_file::file_and_branch(path)?;
        Some(load_project_file(daw, path, &file, &branch, media))
    }

    #[cfg(feature = "native")]
    fn save_session(&self, opened: &Opened, to: &Path) -> Option<eyre::Result<std::path::PathBuf>> {
        Some(
            crate::session_file::save_session(&opened.daw, &opened.project_guid, to)
                .map_err(|e| eyre::eyre!(e)),
        )
    }
}

/// Load exactly `path` (a `.RPP` or a `.session`) into `daw` — the
/// runtime's `load_into`, opening a project file the engine's way.
///
/// # Errors
///
/// The file could not be read or parsed, or its media did not materialize.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_into(daw: &Standalone, path: &Path) -> eyre::Result<Opened> {
    session_runtime::open_core::load_into(daw, path, &Prepare::default())
}

/// [`load_into`], choosing when the media loads.
///
/// # Errors
///
/// The file could not be read or parsed, or (with [`Media::All`]) its
/// media did not materialize.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_into_with(daw: &Standalone, path: &Path, media: Media) -> eyre::Result<Opened> {
    session_runtime::open_core::load_into_with(daw, path, media, &Prepare::default())
}

/// [`load_into_with`] for a file in `folder`.
///
/// # Errors
///
/// As [`load_into_with`].
pub fn load_in(
    folder: &dyn Folder,
    daw: &Standalone,
    path: &Path,
    media: Media,
) -> eyre::Result<Opened> {
    session_runtime::open_core::load_in(folder, daw, path, media, &Prepare::default())
}

/// A project file opened (`load_in` for one): `branch` of `file` rebuilt in the
/// engine from its doc, its media anchored and (with [`Media::All`])
/// materialized as an `.RPP`'s are.
///
/// # Errors
///
/// The file would not open, or its media did not materialize.
#[cfg(feature = "native")]
fn load_project_file(
    daw: &Standalone,
    path: &Path,
    file: &Path,
    branch: &str,
    media: Media,
) -> eyre::Result<Opened> {
    let media_dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
    daw.media_bay().set_file_resolver(Box::new(
        daw::standalone::media_bay::ProjectRelativeResolver::new(media_dir.clone()),
    ));
    let name = if branch == session_format::store::MAIN {
        file.file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
    } else {
        branch.to_owned()
    };
    let mut stages = crate::stages::Stages::start();
    let project_guid = crate::project_file::load(daw, &name, file, branch)
        .map_err(|e| eyre::eyre!("{} did not open: {e}", path.display()))?;
    let build_ms = stages.mark("load.build_ms");
    daw::standalone::project_loader::anchor_media(daw, &project_guid, &media_dir);
    if media == Media::All {
        let audio =
            daw::standalone::audio_engine::materialize::materialize_via_bay(daw, &project_guid)
                .map_err(|e| eyre::eyre!("{name}'s media did not materialize: {e}"))?;
        if !audio.failed.is_empty() {
            tracing::warn!(
                failed = audio.failed.len(),
                loaded = audio.loaded,
                "some sources did not materialize"
            );
        }
    }
    let media_ms = stages.mark("load.media_ms");
    let track_count = daw::service::Tracks::all(
        daw,
        daw::service::ProjectContext::Project(project_guid.clone()),
    )
    .len();
    tracing::info!(
        load.name = %name,
        load.tracks = track_count,
        load.build_ms = build_ms,
        load.media_ms = media_ms,
        "project file opened"
    );
    Ok(Opened {
        daw: daw.clone(),
        name,
        project_guid,
        track_count,
    })
}
