// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// `unused` is paid down (#138); dead code is next, each item a judgement
// call between deleting it and wiring it up.
#![allow(dead_code)]

//! Session Services Library
//!
//! This crate provides session management services for FTS Control.
//! Services can be used either:
//! - As a cell (via `main.rs` and `run_cell!` macro)
//! - In-process (by importing and instantiating directly)
//!
//! # In-Process Usage
//!
//! ```rust,ignore
//! use crate::SessionServices;
//! use daw::Services;
//!
//! // Create a router containing the canonical session service bundle.
//! let router = SessionServices.into_router();
//! ```

#[cfg(feature = "library")]
pub mod library;
/// A project file synced through the org that owns it.
#[cfg(all(feature = "project-sync", not(target_arch = "wasm32")))]
pub mod project_sync;
pub mod proto;
pub mod sync;
#[cfg(feature = "ui")]
pub mod ui;
/// Task vault notes and live-setlist projects from the Song Library
/// (the `session-vault-sync` and `session-player` tools).
#[cfg(all(feature = "vault-sync", not(target_arch = "wasm32")))]
pub mod vault_sync;

// Re-export session-proto so app crates can use `crate::` instead of
// `crate::proto::` directly. `setlist` / `song` are NOT re-exported here:
// this crate has its own domain folders by those names, and each one
// re-exports the matching proto module itself so `crate::setlist::…`
// still resolves to the proto types.
pub use crate::proto::*;
pub use crate::proto::{offset_map, ruler_lanes, services, track_structure};

// ── Domain folders ──────────────────────────────────────────────────────
//
// One per domain, each pairing the implementation with a re-export of its
// `session_proto` counterpart. The setlist stack drives the
// backend-agnostic `daw::get()` facade over tokio primitives that are all
// wasm-safe (the browser setlist engine builds/serves the same
// `SetlistServiceImpl` in-process). Only the REAPER coupling was the wasm
// blocker, now routed through `daw_proto::main_thread` (inline on
// non-REAPER backends). See setlist::service::live_daw_sync (native-only —
// the SynchronizationEngine is REAPER-linked).
pub mod comping;
pub mod count_in;
pub mod reference;
pub mod review;
/// What a section region or a marker is: the session primitive's.
pub use fts_session::section_kinds;
pub mod setlist;
pub mod song;
pub mod strip_width;

// REAPER-hotkey action domains driving the `studio::reaper` backend directly.
// Not needed by the browser setlist engine — native-only. `track_manager`
// also drives `dynamic-template` (the native template engine).
#[cfg(all(not(target_arch = "wasm32"), feature = "reaper"))]
pub mod color;
pub mod guide;
pub mod key;
#[cfg(not(target_arch = "wasm32"))]
pub mod key_actions;
pub mod key_change;
pub mod load_selection;
// NOT gated, deliberately: `chart_import`, `setlist::service::demo` and
// `task-player-ui` (the browser setlist engine) all reach into
// `keyflow::actions`. Adding gated modules directly above this line once
// left its `#[cfg]` attached to `keyflow` instead, which silently
// wasm-gated it and broke the task-web image build — so keep a
// non-attribute line between this and any gated module added above.
pub mod keyflow;
pub mod lyrics;
/// The phases a mix goes through: the session primitive's.
pub use fts_session::mix_phases;
pub mod mode_phases;
pub mod modes;
pub mod open_link;
pub mod playback;
// Growing a guitar part over the daw facade. Native-only for the same
// reason as `track_manager`: it drives `dynamic-template`.
#[cfg(not(target_arch = "wasm32"))]
pub mod guitar_grow;
// REAPER-side helper (routing-project mutation). Not needed by the browser
// build; kept native-only.
#[cfg(not(target_arch = "wasm32"))]
pub mod routing;
#[cfg(not(target_arch = "wasm32"))]
pub mod track_manager;

// ── Infrastructure ──────────────────────────────────────────────────────
//
// Cross-domain plumbing, deliberately flat.
//
// Pre-roll, record control, track grouping, take ranking and auto-colour
// moved to the `daw-actions` crate: they're plain DAW operations with
// nothing session-specific about them.
pub mod cache;
pub mod event_bus;

// Re-export service implementations for library use
pub use setlist::SetlistServiceImpl;

/// Collaboration: the session as a Loro doc, presence, shared transport.
#[cfg(not(target_arch = "wasm32"))]
pub use song::SongServiceImpl;

// Re-export builders for advanced use cases
pub use setlist::SetlistBuilder;
pub use song::SongBuilder;

// Re-export demo setlist stamping (for extensions that have a local Daw instance)
pub use setlist::service::demo::{stamp_demo_into_project, stamp_demo_setlist};

/// Register every REAPER action this crate declares against `backend`.
///
/// One call instead of six. A module added here reaches every host that
/// already calls this, rather than silently not registering until each
/// call site remembers to add a line — which is exactly how
/// `FTS_SESSION_TOGGLE_PLAYBACK` sat dead in REAPER's action list.
///
/// Scope nesting is composed here, not declared by the leaf traits:
/// `track_manager` names only itself ("Track Manager"), `guitar_grow`
/// only "Guitar Grow", and both get wrapped in a `SESSION`-scoped
/// backend on the way in.
///
/// `daw` is the backend the handlers drive (`studio::reaper::Reaper` in
/// production, `daw_standalone::sync::Standalone` in tests).
#[cfg(not(target_arch = "wasm32"))]
pub fn register_all_actions<D, B>(backend: &B, daw: D)
where
    D: daw::service::Projects
        + daw::service::transport::service::Transport
        + daw::service::Markers
        + daw::service::Regions
        + daw::service::TempoMap
        + daw::service::Tracks
        + daw::service::Items
        + daw::service::Takes
        + daw::service::Effects
        + daw::service::Midi
        + daw::service::PositionConversion
        + daw::service::UiDialogs
        + Clone
        + Send
        + Sync
        + 'static,
    B: architect::action::ActionBackend + Clone,
{
    #[cfg(feature = "reaper")]
    color::register_actions(backend);
    guide::register_actions(backend, daw.clone());
    key_actions::register_actions(backend, daw.clone());
    keyflow::actions::register_actions(backend, daw.clone());
    keyflow::scaffold::register_actions(backend, daw.clone());
    #[cfg(feature = "reaper")]
    modes::register_actions(backend);
    playback::register_actions(backend, daw.clone());
    setlist::actions::register_actions(backend, daw.clone());
    crate::proto::track_manager::register_track_manager_actions(
        // "FTS_SESSION", not "SESSION": every other command in the tree
        // carries the FTS_ prefix, and reaper-input's tracks.styx binds
        // `_FTS_SESSION_TRACK_MANAGER_ADD_*`. Registering as plain
        // `SESSION_*` left all five of those keybindings resolving to
        // nothing.
        &architect::action::ScopedActionBackend::new(backend.clone(), "FTS_SESSION", "Session"),
        std::sync::Arc::new(track_manager::TrackManager::new(daw.clone())),
    );
    crate::proto::guitar_grow::register_guitar_grow_actions(
        // Same scope as the track manager: these are Session commands
        // too, and the trait says nothing about being nested under one.
        &architect::action::ScopedActionBackend::new(backend.clone(), "FTS_SESSION", "Session"),
        std::sync::Arc::new(guitar_grow::GuitarGrow::new(daw)),
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, Default)]
pub struct SessionServices;

#[cfg(not(target_arch = "wasm32"))]
impl SessionServices {
    /// Build the setlist/song/MIDI-chart services, sharing one
    /// `SetlistServiceImpl` between the RPC mount and the action-handler
    /// chain (so the REAPER `build_setlist` hotkey and `fts session setlist`
    /// see the same in-memory state — it's `Clone` over `Arc`'d fields, so
    /// cloning gives a handle to the same setlist / `song_cache` / etc).
    #[allow(clippy::type_complexity)]
    fn mounted_service_triple<D>(daw: D) -> (daw::Mounted, daw::Mounted, daw::Mounted, daw::Mounted)
    where
        D: Clone
            + daw::service::AudioEngine
            + daw::service::ExtState
            + daw::service::Projects
            + daw::service::TempoMap
            + daw::service::transport::service::Transport
            + daw::service::Tracks
            + Send
            + Sync
            + 'static,
    {
        use keyflow_daw_analysis::{
            KeyflowMidiAnalysis, MidiChartsDispatcher, midi_charts_service_descriptor,
        };
        // The stream sibling (`events` / `active_indices` `#[subscribe]`
        // hubs) was never mounted here — every consumer of
        // `layer_services_with_daw` (fts-extensions included) got the
        // plain setlist RPC but NOT its live-update streams, so a remote
        // client's `SetlistServiceStreamClient::active_indices`/`events`
        // subscribe calls silently found no such service. Live Mode's
        // own `SessionEngine::router()` mounts this sibling by hand for
        // exactly this reason; doing it here means every consumer gets
        // it for free instead of needing to remember to.
        use services::setlist_service::{
            setlist_service_stream_service_descriptor, stream_serve as setlist_service_stream_serve,
        };

        let setlist_impl = SetlistServiceImpl::with_daw(daw);
        setlist::actions::register(&setlist_impl);
        (
            daw::Mounted::new(
                setlist_service_service_descriptor(),
                serve_setlist_service(setlist_impl.clone()),
            ),
            daw::Mounted::new(
                setlist_service_stream_service_descriptor(),
                setlist_service_stream_serve(setlist_impl),
            ),
            daw::Mounted::new(
                song_service_service_descriptor(),
                serve_song_service(SongServiceImpl::new()),
            ),
            daw::Mounted::new(
                midi_charts_service_descriptor(),
                MidiChartsDispatcher::new(KeyflowMidiAnalysis::from_global_daw()),
            ),
        )
    }

    pub fn mounted_services_with_daw<D>(daw: D) -> Vec<daw::Mounted>
    where
        D: Clone
            + daw::service::AudioEngine
            + daw::service::ExtState
            + daw::service::Projects
            + daw::service::TempoMap
            + daw::service::transport::service::Transport
            + daw::service::Tracks
            + Send
            + Sync
            + 'static,
    {
        let (setlist, setlist_stream, song, midi) = Self::mounted_service_triple(daw);
        vec![setlist, setlist_stream, song, midi]
    }

    /// Builds a composite layer from session services with the given DAW backend.
    pub fn mounted_layers_with_daw<D>(daw: D) -> impl daw::Layer<Self>
    where
        D: Clone
            + daw::service::AudioEngine
            + daw::service::ExtState
            + daw::service::Projects
            + daw::service::TempoMap
            + daw::service::transport::service::Transport
            + daw::service::Tracks
            + Send
            + Sync
            + 'static,
    {
        let (setlist, setlist_stream, song, midi) = Self::mounted_service_triple(daw);
        daw::layers![setlist, setlist_stream, song, midi]
    }

    pub fn merge_into_with_daw<D>(mut handler: daw::LayerRouter, daw: D) -> daw::LayerRouter
    where
        D: Clone
            + daw::service::AudioEngine
            + daw::service::ExtState
            + daw::service::Projects
            + daw::service::TempoMap
            + daw::service::transport::service::Transport
            + daw::service::Tracks
            + Send
            + Sync
            + 'static,
    {
        for service in Self::mounted_services_with_daw(daw) {
            handler = handler.merge(service);
        }
        handler
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod daw_services {
    /// Add session-owned DAW-adjacent services to an in-process DAW router.
    ///
    /// The extension host owns the REAPER transport/channel. Session owns the
    /// chart-analysis service because setlist hydration is its consumer and the
    /// implementation depends on keyflow domain logic.
    pub fn layer_services_with_daw<D>(handler: daw::LayerRouter, daw: D) -> daw::LayerRouter
    where
        D: Clone
            + daw::service::AudioEngine
            + daw::service::ExtState
            + daw::service::Projects
            + daw::service::TempoMap
            + daw::service::transport::service::Transport
            + daw::service::Tracks
            + Send
            + Sync
            + 'static,
    {
        crate::SessionServices::merge_into_with_daw(handler, daw)
    }
}
