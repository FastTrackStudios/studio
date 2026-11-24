//! On the web there is no in-process tokio runtime: the engine clients
//! (`engine::Transport`, `Applier`, `Meters`, …) find none and run
//! read-only, the way they do natively before a session is open. The web
//! host drives the engine its own way (`web_host`).

pub use session_runtime::open_core::{
    AsItIs, Media, OpenHost, Opened, ProjectText, SongPlan, is_session, project_text,
    project_text_in,
};

/// Never constructed: [`runtime`] has none to give.
pub struct Runtime(());

impl Runtime {
    pub fn block_on<F: core::future::Future>(&self, _future: F) -> F::Output {
        unreachable!("no runtime is ever handed out on the web")
    }
}

#[must_use]
pub fn runtime() -> Option<&'static Runtime> {
    None
}

/// The song on screen: a page shows one ([`set_current_song`]).
#[must_use]
pub fn current_song() -> Option<String> {
    CURRENT.read().ok().and_then(|slot| slot.clone())
}

static CURRENT: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

/// The page shows `project` now.
pub fn set_current_song(project: &str) {
    if let Ok(mut slot) = CURRENT.write() {
        *slot = Some(project.to_owned());
    }
}

/// Make `project_guid` the song on screen — what picking a setlist tab
/// does; on a page, [`crate::web_engine::switch_song`].
pub fn switch_song(project_guid: &str) {
    #[cfg(feature = "web")]
    crate::body::body().web_switch_song(project_guid);
    #[cfg(not(feature = "web"))]
    set_current_song(project_guid);
}

/// What needs the local engine's own handle (as natively).
#[derive(Clone, Copy, Debug)]
pub enum LocalOnly {
    ChartRebuild,
    SessionSave,
    /// daw-transport-sync's per-buffer backend: a page's audio loop has
    /// none yet, so it follows a shared transport by the coarse
    /// commands (a play, a seek) rather than to the sample.
    TransportSync,
}

/// The local engine's handle, for what needs it — none on a page yet.
pub fn with_local_engine<R>(
    _what: LocalOnly,
    _f: impl FnOnce(&daw::standalone::Standalone) -> R,
) -> Option<R> {
    None
}
