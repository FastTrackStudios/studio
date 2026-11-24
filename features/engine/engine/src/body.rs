//! The engine's body, as the window modules reach it (`daw_view::body`,
//! and Studio's half, `session_engine_client::body::StudioBody`):
//! installed by [`install`] as the engine
//! comes up in a process — its first open, its runtime, its Action List.

use session_engine_client::body::{Body, StudioBody};

/// The engine in this process.
struct EngineBody;

impl Body for EngineBody {
    fn poi_refresh(&self) {
        crate::poi::refresh();
    }

    #[cfg(feature = "native")]
    fn overlay_read_current(&self) {
        if let Ok((daw, project)) = crate::actions::song()
            && let daw::service::ProjectContext::Project(song) = &project
        {
            let mut overlay = crate::hits::overlay(daw, &project);
            overlay.marks.sort_by(|a, b| a.time.total_cmp(&b.time));
            crate::studio_engine::view_host::overlay_read(song.clone(), overlay);
        }
    }

    #[cfg(feature = "native")]
    fn view_changed(&self, song: Option<String>, what: crate::view::Change) {
        crate::studio_engine::view_host::changed(song, what);
    }

    #[cfg(feature = "native")]
    fn view_changed_items(
        &self,
        song: Option<String>,
        what: crate::view::Change,
        items: Vec<String>,
    ) {
        crate::studio_engine::view_host::changed_items(song, what, items);
    }

    #[cfg(any(feature = "native", feature = "web"))]
    fn transport_pressed(&self) {
        crate::collab::transport_pressed();
    }

    #[cfg(feature = "native")]
    fn record_toggle(&self) {
        crate::record::toggle_through_the_list();
    }

    #[cfg(feature = "native")]
    fn readiness(&self, project: &str) -> Option<session_engine_client::body::Readiness> {
        crate::stream_set::readiness(project)
    }

    #[cfg(feature = "native")]
    fn is_stopped(&self, project: &str) -> bool {
        crate::stream_set::is_stopped(project)
    }

    #[cfg(feature = "native")]
    fn vocal_has_lanes(
        &self,
        daw: &daw::standalone::Standalone,
        project: &daw::service::ProjectContext,
        track: &str,
    ) -> bool {
        crate::vocal_level::has_lanes(daw, project, track)
    }

    #[cfg(feature = "native")]
    fn vocal_lane_shown(
        &self,
        daw: &daw::standalone::Standalone,
        project: &daw::service::ProjectContext,
        track: &str,
    ) -> bool {
        crate::vocal_level::lane_shown(daw, project, track)
    }

    #[cfg(feature = "native")]
    fn vocal_leveled(
        &self,
        daw: &daw::standalone::Standalone,
        project: &daw::service::ProjectContext,
        track: &daw_proto::Track,
    ) -> Vec<crate::automation::Leveled> {
        crate::vocal_level::leveled(daw, project, track)
    }

    #[cfg(feature = "web")]
    fn web_run(
        &self,
        id: String,
        args: Vec<u8>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, String>>>> {
        #[cfg(not(feature = "native"))]
        return Box::pin(async move { crate::web_actions::run(&id, &args).await });
        #[cfg(feature = "native")]
        return Box::pin(
            async move { Err(format!("{id} ({} bytes): no page list here", args.len())) },
        );
    }

    #[cfg(feature = "web")]
    fn web_partial(&self, song: &str, guid: &str) -> Option<bool> {
        crate::web_engine::with_engine(|daw| {
            crate::midi::streaming_partial(
                daw,
                &daw::service::ProjectContext::Project(song.to_owned()),
                guid,
            )
        })
    }
}

impl StudioBody for EngineBody {
    #[cfg(feature = "native")]
    fn switch_local(&self, project: &str) {
        crate::open::switch_local(project);
    }

    #[cfg(feature = "native")]
    fn start_engine(&self) -> eyre::Result<()> {
        crate::open::start_engine().map(|_| ())
    }

    #[cfg(feature = "native")]
    fn cue_start(&self) {
        crate::stream_in::start();
    }

    #[cfg(feature = "native")]
    fn chart_context(
        &self,
    ) -> Option<(
        session::sync::SessionDoc,
        std::sync::Arc<dyn session::sync::net::PresenceSink>,
        String,
    )> {
        crate::collab::chart_context()
    }

    #[cfg(feature = "web")]
    fn web_kinds_of(&self, project: &str) -> Option<crate::plan::Kinds> {
        crate::web_engine::kinds_of(project)
    }

    #[cfg(feature = "web")]
    fn web_switch_song(&self, project: &str) {
        crate::web_engine::switch_song(project);
    }
}

static BODY: EngineBody = EngineBody;

/// Install the engine's body for the window modules in this process, and
/// its actions into the Action List: once, as the engine comes up (its
/// first open, its runtime, the list's first use). Idempotent.
pub fn install() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        session_engine_client::body::install(&BODY);
        session_engine_client::body::install_studio(&BODY);
        #[cfg(feature = "native")]
        session_engine_client::actions::fill_with(crate::actions::fill);
    });
}
