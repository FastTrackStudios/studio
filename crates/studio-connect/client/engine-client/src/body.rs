//! What a Studio window needs of the engine's body, when the engine runs in
//! this process.
//!
//! The window's modules reach the body through `daw_view::body` (re-exported
//! here): tell the linked windows something changed, read the hits again,
//! ask how far a streamed song can play. Studio's own reach it through
//! [`StudioBody`], which the engine installs beside it (`studio_engine::install`):
//! switch the song this process's engine plays, start the engine, start
//! the Cue task, the live session's chart, a page's song. A window linked to
//! an engine in another process installs neither; each call then does what
//! a window with no engine of its own does.

use std::sync::OnceLock;

pub use daw_view::body::*;

/// Studio's half of the engine's body. Every method has the answer of a
/// process with no engine in it as its default.
pub trait StudioBody: Send + Sync {
    /// Make `project` the song this process's engine plays
    /// (`studio_engine::open::switch_to`, with the audio when it plays).
    fn switch_local(&self, project: &str) {
        tracing::warn!(
            song.project = project,
            "no engine in this process to switch the song on"
        );
    }

    /// Stand this process's engine up with no song open
    /// (`studio_engine::open::start_engine`).
    ///
    /// # Errors
    ///
    /// There is no engine in this build, or it could not be started.
    fn start_engine(&self) -> eyre::Result<()> {
        eyre::bail!("this build runs no engine of its own")
    }

    /// Start the Cue task (`studio_engine::stream_in::start`): a window
    /// driving another system playing its click and guide here.
    fn cue_start(&self) {}

    /// The live session's chart, for the caret's presence
    /// (`studio_engine::collab::chart_context`).
    #[cfg(feature = "native")]
    fn chart_context(
        &self,
    ) -> Option<(
        session::sync::SessionDoc,
        std::sync::Arc<dyn session::sync::net::PresenceSink>,
        String,
    )> {
        None
    }

    /// The page's engine's track kinds for `project`
    /// (`studio_engine::web_engine::kinds_of`).
    #[cfg(feature = "web")]
    fn web_kinds_of(&self, _project: &str) -> Option<crate::plan::Kinds> {
        None
    }

    /// Make `project` the page's song (`studio_engine::web_engine::switch_song`).
    #[cfg(feature = "web")]
    fn web_switch_song(&self, project: &str) {
        crate::open::set_current_song(project);
    }
}

/// A process with no engine of its own.
struct Absent;

impl StudioBody for Absent {}

static STUDIO_BODY: OnceLock<&'static dyn StudioBody> = OnceLock::new();

/// Install the engine's Studio half: once, by the engine, as it comes up.
pub fn install_studio(body: &'static dyn StudioBody) {
    let _ = STUDIO_BODY.set(body);
}

/// The engine's Studio half, or a process's with none.
#[must_use]
pub fn studio_body() -> &'static dyn StudioBody {
    STUDIO_BODY.get().copied().unwrap_or(&Absent)
}
