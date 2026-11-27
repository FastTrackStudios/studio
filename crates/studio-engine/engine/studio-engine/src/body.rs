//! The engine's body, as the window modules reach it (`daw_view::body`,
//! and Studio's half, `engine_runtime::client::body::StudioBody`):
//! installed by [`install`] as the engine
//! comes up in a process — its first open, its runtime, its Action List.

use engine_runtime::client::body::{Body, StudioBody};

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
    fn readiness(&self, project: &str) -> Option<engine_runtime::client::body::Readiness> {
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
        engine_sync::SessionDoc,
        std::sync::Arc<dyn engine_sync::net::PresenceSink>,
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
/// What the core asks of the engine around it (`engine_core::assembly`):
/// the domains' parts of an open, a step and a prepared song.
#[cfg(feature = "native")]
static ASSEMBLY: engine_core::assembly::Assembly = engine_core::assembly::Assembly {
    install,
    serve_mcp: || {
        let _ = crate::studio_engine::mcp::start();
    },
    clap_host: crate::clap_host::install,
    view_router: crate::studio_engine::view_host::router,
    audio_attached: |daw, stats, rate| {
        crate::audio_health::watch(daw.clone(), std::sync::Arc::clone(&stats), rate);
        crate::perf_meter::watch(daw.clone(), stats, rate);
    },
    step_finished: |label| {
        // A CLAP's `hits:` subscriptions follow each step.
        crate::clap_host::step_finished();
        crate::agent::step_finished(label);
    },
    samples_follow: |daw, project, sync| {
        let mode = match sync {
            engine_core::assembly::StepSync::Follow => crate::hits_midi::Mode::Follow,
            engine_core::assembly::StepSync::Reconcile => crate::hits_midi::Mode::Reconcile,
        };
        crate::actions::samples_follow(daw, project, mode);
    },
    mix_chains: |daw, project| {
        let chains = crate::mix_chains::add(daw, project)?;
        tracing::info!(
            tone = chains.tone,
            buses = chains.buses,
            process = chains.process,
            gates = ?chains.gates,
            one_notes = ?chains.one_notes,
            returns = chains.returns,
            mics = ?chains.mics,
            unknown = ?chains.unknown,
            "prepare: mix chains"
        );
        Ok(())
    },
    reference: |daw, project| {
        let reference = crate::instrument_tracks::file_reference(daw, project)
            .map_err(|e| eyre::eyre!("reference: {e}"))?;
        let by_reference = crate::instrument_tracks::settle_hearing(daw, project)
            .map_err(|e| eyre::eyre!("reference: {e}"))?;
        tracing::info!(
            reference = ?reference,
            heard_by_reference = by_reference,
            "prepare: the song's reference"
        );
        Ok(())
    },
    mark_saved: crate::projects::mark_saved,
    // ── engine-listen's parts ──
    render_span: crate::mix_measure::render_span,
    signal_analyze: crate::signal_offline::analyze,
    signal_after_step: crate::signal_offline::after_step,
    publish_perf: crate::studio_engine::view_host::perf,
    last_perf: crate::studio_engine::view_host::last_perf,
    // ── engine-edit's parts ──
    // ── engine-mix's parts ──
    place: crate::algorithms::place,
    plan: crate::algorithms::plan,
    signal_chain: crate::clap_host::signal::chain,
    signal_run: crate::clap_host::signal::run,
    drum_returns: crate::mix_chains::drum_returns,
    // ── engine-project's parts ──
    streams_read: crate::studio_engine::view_host::streams_read,
    song_opened: crate::collab::song_opened,
    open_path: |path| crate::studio_engine::set::open_path(path).map_err(|e| e.to_string()),
    // ── engine-collab's parts ──
    local_chart: crate::collab::local_chart,
    join_task_in_background: crate::collab::join_task_in_background,
};

pub fn install() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        #[cfg(feature = "native")]
        engine_core::assembly::install(&ASSEMBLY);
        engine_runtime::client::body::install(&BODY);
        engine_runtime::client::body::install_studio(&BODY);
        // And Studio behind the window modules in this process.
        engine_runtime::client::host::install();
        #[cfg(feature = "native")]
        engine_runtime::client::actions::fill_with(crate::actions::fill);
        // Signal, compiled in: a song's Signal (`Signal Dev`, a bundle's
        // path from any machine) is this build's, hosted in-process as its
        // CLAP, with nothing installed, found or loaded from disk.
        #[cfg(feature = "native")]
        daw_standalone::plugin::register_builtin_clap(daw_standalone::plugin::BuiltinClap {
            names: signal_clap::is_signal,
            entry: &signal_clap::entry::ENTRY,
        });
    });
}
