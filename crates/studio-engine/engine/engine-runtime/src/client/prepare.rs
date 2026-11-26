//! Where a song's chart is, beside its project — what the read-back
//! shows. Preparing a song (organize, chart, guide) is the engine's
//! (`studio_engine::prepare`).

use std::path::PathBuf;

/// The one `.kf` chart in the project's folder, if there is exactly one.
/// Two would be a guess, and a wrong chart is worse than none.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn chart_beside(project: &std::path::Path) -> Option<PathBuf> {
    crate::client::folder::Folder::only_with_extension(
        &crate::client::folder::Disk,
        project.parent()?,
        "kf",
    )
}
