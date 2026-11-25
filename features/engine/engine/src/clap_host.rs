//! Studio as the host half of FastTrackStudio's CLAP extensions
//! (signal#26, `daw-clap-ext`): every CLAP the engine loads is offered
//! `com.fasttrackstudio.host-actions/1` over the Action List
//! ([`bridge`]), its subscriptions are fed from the song ([`topics`]),
//! and a Signal instance is read and driven through its
//! `com.fasttrackstudio.signal/1` by track ([`signal`]: `SIGNAL_CHAIN`,
//! `SIGNAL_RUN`, `SIGNAL_SESSION`, `SIGNAL_FEED`), and every CLAP may write
//! the lanes of its own parameters through
//! `com.fasttrackstudio.host-automation/1` ([`automation`]: Signal's
//! offline blocks' results, written at once, no playback).
//!
//! ## The main thread
//!
//! The engine has no GUI main thread: daw's CLAP host calls a control
//! thread holding a plugin's own lock its main thread. So every
//! main-thread call into a plugin is made under that lock
//! (`Standalone::with_plugin_instance`), and the [`pump`] — a thread of
//! its own, every [`TICK`] — is the engine's main loop for plugins: it
//! runs what a plugin asked its main thread to (`request_callback`),
//! notices a parameter rescan (`param_info_revision`), and delivers the
//! events a plugin subscribed to.
//!
//! A plugin calls host-actions from inside such a call (its
//! `on_main_thread`, a `run_action` Studio made), with its own lock held.
//! An action that touched that plugin would wait for the lock forever, so
//! an action never runs on the calling thread: it runs on a worker, and
//! the call waits for it a little ([`bridge::SYNC_WAIT`]). Past that, the
//! answer is `{"job": <id>}`, and the result comes as a `job` event.

pub mod automation;
pub mod bridge;
pub mod signal;
pub mod timeline;
pub mod topics;

#[cfg(test)]
mod tests;

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use daw::standalone::Standalone;

/// How often the [`pump`] runs: a plugin's main-thread request waits at
/// most this long.
pub const TICK: Duration = Duration::from_millis(50);

thread_local! {
    /// How deep this thread is in a main-thread call into a plugin (a
    /// plugin lock held): a host-actions call made from there must not
    /// wait long — the plugin's track is bypassed while its lock is held.
    static IN_PLUGIN: Cell<u32> = const { Cell::new(0) };
}

/// Run `f`, a main-thread call into a plugin made with its lock held,
/// marked as such for [`in_plugin_call`].
pub fn plugin_call<R>(f: impl FnOnce() -> R) -> R {
    struct Out;
    impl Drop for Out {
        fn drop(&mut self) {
            IN_PLUGIN.with(|d| d.set(d.get().saturating_sub(1)));
        }
    }
    IN_PLUGIN.with(|d| d.set(d.get().saturating_add(1)));
    let _out = Out;
    f()
}

/// Whether this thread is inside [`plugin_call`].
#[must_use]
pub fn in_plugin_call() -> bool {
    IN_PLUGIN.with(|d| d.get() > 0)
}

/// Set when an edit step finishes: the hits topics read the hits again.
static STEP_FINISHED: AtomicBool = AtomicBool::new(true);

/// An edit step finished on a song (`daw::standalone::steps`): what
/// `hits:` subscriptions are sent after.
pub fn step_finished() {
    STEP_FINISHED.store(true, Ordering::Release);
}

/// Take the "a step finished" flag.
fn take_step() -> bool {
    STEP_FINISHED.swap(false, Ordering::AcqRel)
}

/// Make `daw` the host half: Studio's actions offered to every CLAP it
/// loads from now on, and the [`pump`] started. Once per process; before
/// the first song opens, so the song's plugins get the actions.
pub fn install(daw: &'static Standalone) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let actions: Arc<dyn daw::standalone::clap_ext::host::HostActions> =
            Arc::new(bridge::StudioActions::default());
        daw.set_plugin_load_options(daw::standalone::plugin::LoadOptions {
            actions: Some(actions),
            automation: Some(Arc::new(automation::StudioAutomation::new(daw))),
            // host-timeline/1: each instance reads its own track's MIDI
            // ahead of the playhead (`timeline`).
            timeline: true,
        });
        let spawned = std::thread::Builder::new()
            .name("clap-main".into())
            .spawn(move || {
                loop {
                    pump(daw);
                    std::thread::sleep(TICK);
                }
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, clap.pump = false, "clap: no main-thread pump");
        }
    });
}

/// One turn of the engine's main loop for plugins: each CLAP's
/// main-thread request run, its rescans noticed (and every cached copy of
/// its parameters refreshed), its timeline bound and kept current, the
/// events it subscribed to delivered.
pub fn pump(daw: &'static Standalone) {
    let guids: Vec<String> = daw.with_plugin_map(|m| m.guids().map(str::to_owned).collect());
    let step = take_step();
    let outbox = topics::Outbox::gather(daw, step);
    let mut live = Vec::new();
    for guid in &guids {
        let known = automation::caller_of(guid).is_some();
        let timeline = timeline::for_fx(daw, guid);
        timeline.refresh();
        let turn = daw.with_plugin_instance(guid, |p| {
            plugin_call(|| {
                if !known {
                    // Before its main thread runs: a write it makes there
                    // names a plugin Studio knows.
                    register_plugin(daw, guid, None, p);
                }
                if !p.timeline_bound() {
                    p.bind_timeline(Some(timeline.clone()));
                }
                if p.wants_main_thread() {
                    p.pump_main_thread();
                }
                let sink = p.event_sink();
                if let Some(sink) = &sink {
                    outbox.deliver(sink);
                    timeline.deliver(sink);
                }
                (p.param_info_revision(), sink)
            })
        });
        let Some((revision, sink)) = turn else {
            continue;
        };
        live.extend(sink);
        if rescanned(guid, revision) {
            refresh_params(daw, guid);
            // Its parameters again, for host-automation/1.
            let _ = daw.with_plugin_instance(guid, |p| {
                plugin_call(|| register_plugin(daw, guid, None, p))
            });
        }
    }
    topics::retain_live(&live);
    forget_gone(&guids);
    automation::retain(&guids);
    // An edit step: offline blocks made from audio that moved go stale.
    if step
        && let Ok((engine, project)) = crate::actions::song()
        && std::ptr::eq(engine, daw)
    {
        crate::signal_offline::after_step(daw, &project);
    }
    for (caller, edit) in automation::user_edits(daw) {
        if let Ok(event) = serde_json::to_string(&edit) {
            topics::automation_edited(caller, &event);
        }
    }
    timeline::forget_gone(&guids);
    signal::publish_feed(daw);
}

/// Know `p` (FX `fx_guid`'s live plugin, its lock held by the caller) for
/// host-automation/1: its FX, its track and song (`at`, or found on the
/// song open now), and its parameter ids.
pub fn register_plugin(
    daw: &Standalone,
    fx_guid: &str,
    at: Option<(daw_proto::ProjectContext, String)>,
    p: &mut dyn daw::standalone::plugin::PluginInstance,
) {
    let Some(sink) = p.event_sink() else {
        return;
    };
    let Some((project, track_guid)) = at.or_else(|| {
        signal::chain_of(daw, fx_guid)
            .map(|(project, track)| (daw_proto::ProjectContext::Project(project), track.guid))
    }) else {
        return;
    };
    let ids: Vec<u32> = p.params().iter().map(|i| i.id).collect();
    automation::register(
        sink.plugin_key(),
        automation::Registered {
            fx_guid: fx_guid.to_owned(),
            track_guid,
            project,
            ids,
        },
    );
}

/// Each plugin's parameter-info revision, as last seen.
static PARAM_REVISIONS: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);

/// Whether `fx_guid`'s plugin rescanned since it was last seen (its first
/// sight is no rescan).
fn rescanned(fx_guid: &str, revision: u64) -> bool {
    let mut seen = PARAM_REVISIONS
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let seen = seen.get_or_insert_with(HashMap::new);
    seen.insert(fx_guid.to_owned(), revision)
        .is_some_and(|was| was != revision)
}

/// Forget the plugins no longer loaded.
fn forget_gone(guids: &[String]) {
    let mut seen = PARAM_REVISIONS
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(seen) = seen.as_mut() {
        seen.retain(|g, _| guids.iter().any(|l| l == g));
    }
}

/// A plugin renamed or re-described its parameters: every cached copy of
/// them is read again — the chain's views (an FX event), the automation
/// lanes kept in this process, and whoever watches the list's `fx` topic.
fn refresh_params(daw: &Standalone, fx_guid: &str) {
    let Some((project, track)) = signal::chain_of(daw, fx_guid) else {
        return;
    };
    daw.announce_fx_chain_changed(
        &project,
        daw_proto::FxChainContext::Track(track.guid.clone()),
    );
    if !crate::automation::lanes_of(&track.guid).is_empty() {
        let context = daw_proto::ProjectContext::Project(project.clone());
        crate::automation::replace(crate::automation::read(
            daw,
            &context,
            std::slice::from_ref(&track),
        ));
    }
    crate::actions::list().changed("fx", &track.guid);
    tracing::info!(
        clap.fx = fx_guid,
        clap.track = %track.name,
        "clap: a plugin rescanned its parameters"
    );
}
