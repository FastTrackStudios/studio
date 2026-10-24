//! The active language: one value, project state.
//!
//! `flow.vocals.language.active`: the session has an active language,
//! and every vocal scene follows it. It is project ext-state — the same
//! mechanism `daw_module`'s `CREATE_STATE_SECTION` already uses for
//! per-track create state — read and written through
//! [`daw::service::ExtState`] so the one write works identically over a
//! live REAPER project and over `daw-standalone`, with no backend-
//! specific code on either side of the seam.

use daw::service::{ExtState, ProjectContext};
use daw_proto::DawResult;

use super::language::Language;

/// The ext-state section the active language lives under.
const SECTION: &str = "FTS_LANGUAGE";
/// The one key: the whole point of "one write" is that switching the
/// language never touches more than this.
const KEY: &str = "active";

/// The session's active language, if one has been set.
///
/// `None` when the project has never had a language switch — a session
/// with no vocals, or one nobody has switched yet. Callers that need a
/// language to hide by should treat that as "hide nothing" rather than
/// guessing one.
#[must_use]
pub fn get_active_language<E: ExtState>(ext: &E, project: ProjectContext) -> Option<Language> {
    ext.get_project(project, SECTION, KEY)
        .and_then(|value| Language::parse(&value))
}

/// Switch the active language: one write.
///
/// # Errors
///
/// Returns whatever the backend's `set_project` returns — a project
/// that does not resolve, most likely.
pub fn set_active_language<E: ExtState>(
    ext: &E,
    project: ProjectContext,
    language: Language,
) -> DawResult<()> {
    ext.set_project(project, SECTION, KEY, language.as_str())
}
