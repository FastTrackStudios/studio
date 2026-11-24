//! The host half across a real C ABI: a plugin built in memory (as
//! `daw-clap-ext/tests/roundtrip.rs` builds one) calls Studio's
//! host-actions/1 through its client; and, where this machine has it,
//! the Signal CLAP on a track, read and driven by SIGNAL_CHAIN and
//! SIGNAL_RUN.

use std::ffi::{CStr, c_char, c_void};
use std::sync::{Arc, Mutex, Once};

use architect::action::{ActionMeta, Effect};
use clap_sys::host::clap_host;
use clap_sys::plugin::clap_plugin;
use daw::standalone::clap_ext::host::{
    EventSink, HostActions, HostActionsLookup, HostActionsVtable,
};
use daw::standalone::clap_ext::plugin::HostActionsClient;
use daw::standalone::clap_ext::{ExtError, sys};
use facet::Facet;

use super::bridge::{self, StudioActions};
use super::topics::{self, Topic};

// ── A plugin in memory, with host-events/1 ──────────────────────────────

static GOT: Mutex<Vec<(u64, String, String)>> = Mutex::new(Vec::new());

unsafe extern "C" fn on_event(
    _p: *const clap_plugin,
    sub: u64,
    topic: *const c_char,
    json: *const c_char,
) {
    let (t, j) = unsafe { (CStr::from_ptr(topic), CStr::from_ptr(json)) };
    GOT.lock()
        .unwrap()
        .push((sub, t.to_string_lossy().into(), j.to_string_lossy().into()));
}

static EVENTS: sys::fts_plugin_host_events = sys::fts_plugin_host_events {
    on_event: Some(on_event),
};

unsafe extern "C" fn plugin_ext(_p: *const clap_plugin, id: *const c_char) -> *const c_void {
    if unsafe { CStr::from_ptr(id) } == sys::FTS_EXT_HOST_EVENTS {
        (&raw const EVENTS).cast()
    } else {
        std::ptr::null()
    }
}

/// The one plugin the in-memory host serves.
struct Plugin(clap_plugin);
// SAFETY: a test's plugin, all-static data.
unsafe impl Send for Plugin {}
// SAFETY: as above.
unsafe impl Sync for Plugin {}

static PLUGIN: std::sync::LazyLock<Plugin> = std::sync::LazyLock::new(|| {
    // SAFETY: an all-null clap_plugin is valid; we fill what is used.
    let mut p: clap_plugin = unsafe { std::mem::zeroed() };
    p.get_extension = Some(plugin_ext);
    Plugin(p)
});

// ── A host whose host_data is Studio's actions ──────────────────────────

struct ByHostData;
impl HostActionsLookup for ByHostData {
    unsafe fn lookup(host: *const clap_host) -> Option<Arc<dyn HostActions>> {
        let data = unsafe { (*host).host_data }.cast::<Arc<dyn HostActions>>();
        (!data.is_null()).then(|| unsafe { (*data).clone() })
    }
    unsafe fn event_sink(_host: *const clap_host) -> Option<EventSink> {
        unsafe { EventSink::query(&raw const PLUGIN.0) }
    }
}

unsafe extern "C" fn host_ext(_host: *const clap_host, id: *const c_char) -> *const c_void {
    if unsafe { CStr::from_ptr(id) } == sys::FTS_EXT_HOST_ACTIONS {
        HostActionsVtable::<ByHostData>::as_ptr()
    } else {
        std::ptr::null()
    }
}

/// A host offering `actions`; drop its `host_data` with [`free`].
fn host(actions: Arc<dyn HostActions>) -> clap_host {
    // SAFETY: as above.
    let mut host: clap_host = unsafe { std::mem::zeroed() };
    host.host_data = Box::into_raw(Box::new(actions)).cast();
    host.get_extension = Some(host_ext);
    host
}

fn free(host: &clap_host) {
    drop(unsafe { Box::from_raw(host.host_data.cast::<Arc<dyn HostActions>>()) });
}

// ── Test actions on Studio's list ───────────────────────────────────────

static ECHO: ActionMeta = crate::actions::meta(
    "CLAP_TEST_ECHO",
    "clap_test_echo",
    "Echo",
    "Gives back what it is given (a test's).",
    "Test",
    "",
);
static SLOW: ActionMeta = crate::actions::meta(
    "CLAP_TEST_SLOW",
    "clap_test_slow",
    "Slow",
    "Takes a while, then says so (a test's).",
    "Test",
    "",
);

#[derive(Facet)]
struct Said {
    word: String,
}

fn register_tests() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let list = crate::actions::list();
        list.register(&ECHO, Effect::ReadOnly, |a: Said| {
            Ok::<_, String>(Said { word: a.word })
        })
        .expect("echo");
        list.register(&SLOW, Effect::ReadOnly, |(): ()| {
            std::thread::sleep(std::time::Duration::from_millis(300));
            Ok::<_, String>(Said {
                word: "done".into(),
            })
        })
        .expect("slow");
    });
}

#[test]
fn a_plugin_lists_describes_and_runs_studios_actions() {
    register_tests();
    let studio = Arc::new(StudioActions::default());
    let host = host(studio.clone());
    let client =
        unsafe { HostActionsClient::query(&raw const host) }.expect("Studio offers host-actions/1");

    let list: serde_json::Value = serde_json::from_str(&client.list().expect("a list")).unwrap();
    let ids: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["id"].as_str())
        .collect();
    for id in [
        "CLAP_TEST_ECHO",
        "SIGNAL_CHAIN",
        "SIGNAL_RUN",
        "SIGNAL_SESSION",
        "SIGNAL_FEED",
        "SIGNAL_KIT",
        "SIGNAL_BYPASS",
    ] {
        assert!(ids.contains(&id), "{id} is listed");
    }
    let echo: serde_json::Value =
        serde_json::from_str(&client.describe("CLAP_TEST_ECHO").unwrap()).unwrap();
    assert_eq!(
        echo["description"],
        "Gives back what it is given (a test's)."
    );
    assert!(echo["args_schema"].is_object(), "{echo}");
    assert!(matches!(
        client.describe("NO_SUCH"),
        Err(ExtError::Failed(_))
    ));

    assert_eq!(
        client.run("CLAP_TEST_ECHO", r#"{"word":"hi"}"#).unwrap(),
        r#"{"word":"hi"}"#
    );
    match client.run("CLAP_TEST_ECHO", r#"{"nope":1}"#) {
        Err(ExtError::Failed(why)) => assert!(why.contains("bad arguments"), "{why}"),
        other => panic!("{other:?}"),
    }
    match client.run("NO_SUCH", "{}") {
        Err(ExtError::Failed(why)) => assert_eq!(why, "no action NO_SUCH"),
        other => panic!("{other:?}"),
    }
    free(&host);
}

/// A plugin calling from its own lock (inside a call Studio made) never
/// waits long: a slow action answers `{"job"}`, and its result arrives as
/// a `job` event on the next turn of the pump.
#[test]
fn a_slow_action_from_inside_a_plugin_call_is_a_job() {
    register_tests();
    let studio = Arc::new(StudioActions::default());
    let host = host(studio.clone());
    let client = unsafe { HostActionsClient::query(&raw const host) }.unwrap();
    let sub = client.subscribe("job").expect("a job subscription");

    let answer = super::plugin_call(|| client.run("CLAP_TEST_SLOW", "{}")).unwrap();
    let job: serde_json::Value = serde_json::from_str(&answer).unwrap();
    let id = job["job"].as_u64().expect(&answer);

    // Outside a plugin's lock, the same action is waited for.
    assert_eq!(
        client.run("CLAP_TEST_SLOW", "{}").unwrap(),
        r#"{"word":"done"}"#
    );

    let deadline = web_time::Instant::now() + std::time::Duration::from_secs(5);
    while !bridge::finished_jobs(&studio).contains_key(&id) {
        assert!(web_time::Instant::now() < deadline, "the job finished");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let daw = daw::standalone::Standalone::new();
    let outbox = topics::Outbox::gather(&daw, false);
    let sink = unsafe { EventSink::query(&raw const PLUGIN.0) }.unwrap();
    outbox.deliver(&sink);
    let got = GOT.lock().unwrap().clone();
    let (s, topic, event) = got
        .iter()
        .find(|(s, t, _)| *s == sub && t == "job")
        .expect("the job's event");
    assert_eq!((*s, topic.as_str()), (sub, "job"));
    let event: serde_json::Value = serde_json::from_str(event).unwrap();
    assert_eq!(event["job"], id);
    assert_eq!(event["ok"], true);
    assert_eq!(event["result"]["word"], "done");
    client.unsubscribe(sub);
    free(&host);
}

#[test]
fn topics_are_studios_and_nothing_else() {
    assert_eq!(Topic::parse("transport"), Some(Topic::Transport));
    assert_eq!(Topic::parse("tempo"), Some(Topic::Tempo));
    assert_eq!(Topic::parse("job"), Some(Topic::Job));
    assert_eq!(
        Topic::parse("hits:Drums/Kick"),
        Some(Topic::Hits("Drums/Kick".into()))
    );
    assert_eq!(Topic::parse("hits:"), None);
    assert_eq!(Topic::parse("weather"), None);
    let sink = unsafe { EventSink::query(&raw const PLUGIN.0) }.unwrap();
    assert_eq!(StudioActions::default().subscribe("weather", sink), None);
}

#[test]
fn files_load_by_reference_and_never_leave_the_library() {
    let root = std::env::temp_dir().join(format!("clap-host-lib-{}", std::process::id()));
    std::fs::create_dir_all(root.join("guitar/models")).unwrap();
    let file = root.join("guitar/models/amp.nam");
    std::fs::write(&file, [1_u8, 2, 3]).unwrap();
    let roots = vec![std::path::PathBuf::from("/nowhere"), root.clone()];
    assert_eq!(
        bridge::resolve_file("rig-library:guitar/models/amp.nam", &roots).unwrap(),
        file
    );
    assert!(bridge::resolve_file("rig-library:../etc/passwd", &roots).is_err());
    assert!(bridge::resolve_file("rig-library:guitar/none.nam", &roots).is_err());
    assert_eq!(
        bridge::resolve_file(&file.to_string_lossy(), &roots).unwrap(),
        file
    );
    assert!(bridge::resolve_file("relative/amp.nam", &roots).is_err());
    let _ = std::fs::remove_dir_all(&root);
}

// ── The Signal CLAP, where this machine has it ──────────────────────────

/// `~/.clap/Signal Dev.clap` on `Drums/Kick/Sum/In`, offered Studio's
/// actions: SIGNAL_CHAIN reads its chain (from the plugin once, then kept
/// while its revision holds), SIGNAL_RUN runs its own actions, and the feed
/// has its row. An older build without signal/1 says so and stops.
#[test]
fn signal_dev_on_a_track_is_read_and_driven() {
    let Some(clap) = crate::clap_fx::bundle("Signal Dev") else {
        eprintln!("skipped: no Signal Dev.clap on this machine");
        return;
    };
    let daw = daw::standalone::Standalone::new();
    daw.set_plugin_load_options(daw::standalone::plugin::LoadOptions {
        actions: Some(Arc::new(StudioActions::default())),
    });
    let ctx = crate::clap_fx::test_support::drum_song(&daw, "signal-ext");
    let track = crate::actions::resolve(&daw, &ctx, "Drums/Kick/Sum/In").expect("the track");
    let chain = daw::service::FxChainContext::Track(track);
    let fx = crate::clap_fx::add(&daw, &ctx, &chain, &clap, u32::MAX).expect("Signal Dev loads");
    if daw.with_plugin_instance(&fx, |p| p.signal().is_some()) != Some(true) {
        eprintln!("no signal/1: {} is a build without it", clap.display());
        assert!(
            super::signal::chain(&daw, &ctx, "Kick/Sum/In", None)
                .unwrap_err()
                .contains("no signal/1")
        );
        return;
    }
    let first = super::signal::chain(&daw, &ctx, "Kick/Sum/In", None).expect("its chain");
    assert_eq!(first["fx"]["guid"], fx.as_str());
    assert_eq!(first["cached"], false);
    assert!(first["chain"].is_object(), "{first}");
    let again = super::signal::chain(&daw, &ctx, "Kick/Sum/In", None).unwrap();
    assert_eq!(again["revision"], first["revision"]);
    assert_eq!(again["cached"], true, "read once while the revision holds");

    let listed =
        super::signal::run(&daw, &ctx, ("Kick/Sum/In", None), "list", "{}").expect("its actions");
    assert_eq!(listed["fx_guid"], fx.as_str());
    let ran = super::signal::run(&daw, &ctx, ("Kick/Sum/In", None), "SIGNAL_CHAIN", "{}")
        .expect("SIGNAL_CHAIN through run_action");
    assert!(!ran["result"].is_null(), "{ran}");

    let feed = super::signal::feed(&daw, &ctx);
    assert_eq!(feed.fx.len(), 1, "{feed:?}");
    assert_eq!(feed.fx[0].fx_guid, fx);
    assert_eq!(feed.fx[0].track, "Drums/Kick/Sum/In");
    // FX_SET names a Signal slot as the chain does: `@<clap_id>`.
    let clap_ids: Vec<String> = first["chain"]["phases"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|p| p["blocks"].as_array().cloned().unwrap_or_default())
        .flat_map(|b| b["params"].as_array().cloned().unwrap_or_default())
        .filter_map(|p| p["clap_id"].as_str().map(str::to_owned))
        .collect();
    if let Some(id) = clap_ids.first() {
        let slot = crate::fx_actions::slot_named(&daw, &fx, &format!("@{id}"));
        assert!(matches!(slot, Ok(Some(_))), "@{id}: {slot:?}");
    } else {
        eprintln!("this Signal Dev exposes no clap_id yet");
    }
    let session = super::signal::session(&daw, &ctx);
    assert_eq!(session["ports"], 1);
}
