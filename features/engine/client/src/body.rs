//! What a window module needs of the engine's body, when the engine runs in
//! this process.
//!
//! This crate holds what a window is built from; the engine's body
//! (`session-engine`: the set, its actions, placement, comping, the audio)
//! depends on it, never the other way. The few places a window module
//! reaches into the body — tell the linked windows something changed, read
//! the hits again, switch the song this process's engine plays — go through
//! [`Body`], which the engine installs when it comes up in a process
//! (`session_engine::install`). A window linked to an engine in another
//! process (Studio) never installs one: each call then does what a window
//! with no engine of its own does — nothing, or an error that says so —
//! and the window reaches the engine over its link (`crate::view_client`).

use std::sync::OnceLock;

/// The engine's body, as the window modules call it. Every method has the
/// answer of a process with no engine in it as its default.
pub trait Body: Send + Sync {
    /// Make `project` the song this process's engine plays
    /// (`session_engine::open::switch_to`, with the audio when it plays).
    fn switch_local(&self, project: &str) {
        tracing::warn!(
            song.project = project,
            "no engine in this process to switch the song on"
        );
    }

    /// Stand this process's engine up with no song open
    /// (`session_engine::open::start_engine`).
    ///
    /// # Errors
    ///
    /// There is no engine in this build, or it could not be started.
    fn start_engine(&self) -> eyre::Result<()> {
        eyre::bail!("this build runs no engine of its own")
    }

    /// Start the Cue task (`session_engine::stream_in::start`): a window
    /// driving another system playing its click and guide here.
    fn cue_start(&self) {}

    /// Read the points of interest again on the engine
    /// (`session_engine::poi::refresh`).
    fn poi_refresh(&self) {}

    /// Read the open song's hit markers on the engine and show them to
    /// every window (`session_engine::hits::overlay`, `view_host::overlay_read`).
    fn overlay_read_current(&self) {}

    /// Tell every window on this engine that `what` changed
    /// (`session_engine::studio_engine::view_host::changed`).
    fn view_changed(&self, _song: Option<String>, _what: crate::view::Change) {}

    /// [`Self::view_changed`], naming the items
    /// (`view_host::changed_items`).
    fn view_changed_items(
        &self,
        _song: Option<String>,
        _what: crate::view::Change,
        _items: Vec<String>,
    ) {
    }

    /// A transport press here, for the live session to follow
    /// (`session_engine::collab::transport_pressed`).
    fn transport_pressed(&self) {}

    /// Record on or off through the Action List
    /// (`session_engine::record::toggle_through_the_list`).
    fn record_toggle(&self) {}

    /// How far a streamed song can play from its playhead
    /// (`session_engine::stream_set::readiness`); `None` when it is not
    /// streaming here.
    fn readiness(&self, _project: &str) -> Option<crate::stream_set::Readiness> {
        None
    }

    /// Whether `project` is stopped on this process's engine (or not a
    /// song here) (`session_engine::stream_set::is_stopped`).
    fn is_stopped(&self, _project: &str) -> bool {
        true
    }

    /// Whether a track has leveler lanes
    /// (`session_engine::vocal_level::has_lanes`).
    fn vocal_has_lanes(
        &self,
        _daw: &daw::standalone::Standalone,
        _project: &daw::service::ProjectContext,
        _track: &str,
    ) -> bool {
        false
    }

    /// Whether a track's leveler lane is shown
    /// (`session_engine::vocal_level::lane_shown`).
    fn vocal_lane_shown(
        &self,
        _daw: &daw::standalone::Standalone,
        _project: &daw::service::ProjectContext,
        _track: &str,
    ) -> bool {
        false
    }

    /// A track's leveled items (`session_engine::vocal_level::leveled`).
    fn vocal_leveled(
        &self,
        _daw: &daw::standalone::Standalone,
        _project: &daw::service::ProjectContext,
        _track: &daw_proto::Track,
    ) -> Vec<crate::automation::Leveled> {
        Vec::new()
    }

    /// The live session's chart, for the caret's presence
    /// (`session_engine::collab::chart_context`).
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

    /// Run an action on the page's own list (`session_engine::web_actions::run`).
    #[cfg(feature = "web")]
    fn web_run(
        &self,
        id: String,
        _args: Vec<u8>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, String>>>> {
        Box::pin(async move { Err(format!("{id}: no engine in this page")) })
    }

    /// Whether `guid`'s recording is still arriving on the page's own
    /// engine (`session_engine::web_engine::with_engine`); `None` with none.
    #[cfg(feature = "web")]
    fn web_partial(&self, _song: &str, _guid: &str) -> Option<bool> {
        None
    }

    /// The page's engine's track kinds for `project`
    /// (`session_engine::web_engine::kinds_of`).
    #[cfg(feature = "web")]
    fn web_kinds_of(&self, _project: &str) -> Option<crate::plan::Kinds> {
        None
    }

    /// Make `project` the page's song (`session_engine::web_engine::switch_song`).
    #[cfg(feature = "web")]
    fn web_switch_song(&self, project: &str) {
        crate::open::set_current_song(project);
    }
}

/// A process with no engine of its own.
struct Absent;

impl Body for Absent {}

static BODY: OnceLock<&'static dyn Body> = OnceLock::new();

/// Install the engine's body: once, by the engine, as it comes up.
pub fn install(body: &'static dyn Body) {
    let _ = BODY.set(body);
}

/// Whether an engine installed its body in this process.
#[must_use]
pub fn installed() -> bool {
    BODY.get().is_some()
}

/// The engine's body, or a process's with none.
#[must_use]
pub fn body() -> &'static dyn Body {
    BODY.get().copied().unwrap_or(&Absent)
}
