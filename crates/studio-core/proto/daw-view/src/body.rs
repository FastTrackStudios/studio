//! What a window module needs of the engine's body, when the engine runs in
//! this process.
//!
//! This crate is what a window is built from; an engine's body (its
//! actions, placement, comping, the audio) depends on it, never the other
//! way. The few places a window module reaches into the body — tell the
//! linked windows something changed, read the hits again, ask how far a
//! streamed song can play — go through [`Body`], which the engine installs
//! when it comes up in a process. A window linked to an engine in another
//! process never installs one: each call then does what a window with no
//! engine of its own does — nothing, or an error that says so — and the
//! window reaches the engine over its link (`crate::view_client`).

use std::sync::OnceLock;

/// How far a streamed song can play from where it is, every recording it
/// plays there.
#[derive(Clone, Debug, PartialEq, facet::Facet)]
pub struct Readiness {
    /// The song (its engine project).
    pub project: String,
    /// Where it would play from, seconds.
    pub playhead: f64,
    /// Seconds from the playhead every playing recording has.
    pub buffered: f64,
    /// It may play: enough of everything is here.
    pub ready: bool,
    /// Recordings missing something soon after the playhead.
    pub waiting_on: u32,
}

/// The engine's body, as the window modules call it. Every method has the
/// answer of a process with no engine in it as its default.
pub trait Body: Send + Sync {
    /// Read the points of interest again on the engine
    /// (`studio_engine::poi::refresh`).
    fn poi_refresh(&self) {}

    /// Read the open song's hit markers on the engine and show them to
    /// every window (`studio_engine::hits::overlay`, `view_host::overlay_read`).
    fn overlay_read_current(&self) {}

    /// Tell every window on this engine that `what` changed
    /// (`studio_engine::studio_engine::view_host::changed`).
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
    /// (`studio_engine::collab::transport_pressed`).
    fn transport_pressed(&self) {}

    /// Record on or off through the Action List
    /// (`studio_engine::record::toggle_through_the_list`).
    fn record_toggle(&self) {}

    /// How far a streamed song can play from its playhead
    /// (`studio_engine::stream_set::readiness`); `None` when it is not
    /// streaming here.
    fn readiness(&self, _project: &str) -> Option<Readiness> {
        None
    }

    /// Whether `project` is stopped on this process's engine (or not a
    /// song here) (`studio_engine::stream_set::is_stopped`).
    fn is_stopped(&self, _project: &str) -> bool {
        true
    }

    /// Whether a track has leveler lanes
    /// (`studio_engine::vocal_level::has_lanes`).
    #[cfg(feature = "local")]
    fn vocal_has_lanes(
        &self,
        _daw: &daw::standalone::Standalone,
        _project: &daw::service::ProjectContext,
        _track: &str,
    ) -> bool {
        false
    }

    /// Whether a track's leveler lane is shown
    /// (`studio_engine::vocal_level::lane_shown`).
    #[cfg(feature = "local")]
    fn vocal_lane_shown(
        &self,
        _daw: &daw::standalone::Standalone,
        _project: &daw::service::ProjectContext,
        _track: &str,
    ) -> bool {
        false
    }

    /// A track's leveled items (`studio_engine::vocal_level::leveled`).
    #[cfg(feature = "local")]
    fn vocal_leveled(
        &self,
        _daw: &daw::standalone::Standalone,
        _project: &daw::service::ProjectContext,
        _track: &daw_proto::Track,
    ) -> Vec<crate::automation::Leveled> {
        Vec::new()
    }

    /// How many undo steps the engine finished that the song's history has
    /// not recorded yet (`engine_core::project_file::steps_pending`): what
    /// an action waits on before and after its own step, so a fast run of
    /// calls is a step each and not one commit. None with no engine here.
    fn steps_pending(&self) -> usize {
        0
    }

    /// Run an action on the page's own list (`studio_engine::web_actions::run`).
    #[cfg(feature = "web")]
    fn web_run(
        &self,
        id: String,
        _args: Vec<u8>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, String>>>> {
        Box::pin(async move { Err(format!("{id}: no engine in this page")) })
    }

    /// Whether `guid`'s recording is still arriving on the page's own
    /// engine (`studio_engine::web_engine::with_engine`); `None` with none.
    #[cfg(feature = "web")]
    fn web_partial(&self, _song: &str, _guid: &str) -> Option<bool> {
        None
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
