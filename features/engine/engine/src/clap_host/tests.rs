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

/// A second plugin in memory: a render's copy of [`PLUGIN`].
static COPY: std::sync::LazyLock<Plugin> = std::sync::LazyLock::new(|| {
    // SAFETY: as PLUGIN's.
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
        ..daw::standalone::plugin::LoadOptions::default()
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

/// A render's copy of a plugin is subscribed to nothing and the pump never
/// reaches it: it is handed the last event of each of its original's
/// `hits:` subscriptions (and nothing else), so its gates know the song's
/// placed hits as the live instance does.
#[test]
fn a_renders_copy_is_handed_its_originals_placed_hits() {
    let original = unsafe { EventSink::query(&raw const PLUGIN.0) }.unwrap();
    let copy = unsafe { EventSink::query(&raw const COPY.0) }.unwrap();
    assert!(!original.same_plugin(&copy));
    let topic = "hits:Drums/Toms/Tom 4-copy-test";
    let hits = topics::subscribe(Topic::parse(topic).unwrap(), original);
    let transport = topics::subscribe(Topic::Transport, original);
    // Before anything was sent, there is nothing to hand.
    assert_eq!(topics::replay_hits(&original, &copy), 0);
    let event =
        r#"{"piece":"Tom 4","hits":[{"time":1.0,"velocity":100},{"time":16.0,"velocity":100}]}"#;
    topics::record_sent(hits, event);
    topics::record_sent(transport, r#"{"state":"playing"}"#);
    GOT.lock().unwrap().retain(|(_, t, _)| t != topic);
    assert_eq!(topics::replay_hits(&original, &copy), 1, "the hits alone");
    let got: Vec<_> = GOT
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, t, _)| t == topic)
        .cloned()
        .collect();
    assert_eq!(got, vec![(0, topic.to_owned(), event.to_owned())]);
    topics::unsubscribe(hits);
    topics::unsubscribe(transport);
}

/// A plugin subscribes while it loads, before it is on a chain: the pump
/// keeps that subscription until it is, and drops it once the plugin,
/// seen on a chain, is gone from it.
#[test]
fn a_subscription_made_before_its_plugin_is_on_a_chain_is_kept() {
    let loading = unsafe { EventSink::query(&raw const COPY.0) }.unwrap();
    let other = unsafe { EventSink::query(&raw const PLUGIN.0) }.unwrap();
    let topic = Topic::parse("hits:Drums/Toms/Tom 4-retain-test").unwrap();
    let id = topics::subscribe(topic.clone(), loading);
    topics::retain_live(&[other]);
    assert_eq!(
        topics::subscriptions_of(&loading, &topic),
        vec![id],
        "kept while the plugin is not on its chain yet"
    );
    topics::retain_live(&[other, loading]);
    topics::retain_live(&[other]);
    assert!(
        topics::subscriptions_of(&loading, &topic).is_empty(),
        "dropped once its plugin, seen on a chain, is gone"
    );
}

// ── Setting a Signal block's parameters, the transport stopped ──────────

/// The Signal Dev bundle the tests load: `SIGNAL_DEV_CLAP` (a build being
/// tested), else `~/.clap/Signal Dev.clap`.
fn signal_dev() -> Option<std::path::PathBuf> {
    std::env::var_os("SIGNAL_DEV_CLAP")
        .map(std::path::PathBuf::from)
        .filter(|p| p.exists())
        .or_else(|| crate::clap_fx::bundle("Signal Dev"))
}

/// A song with Signal Dev on `Drums/Kick/Sum/In`, activated and never
/// processed (a stopped transport), a Tone EQ and a Tone comp added.
fn stopped_signal(
    song: &str,
) -> Option<(
    daw::standalone::Standalone,
    daw::service::ProjectContext,
    String,
)> {
    let Some(clap) = signal_dev() else {
        eprintln!("skipped: no Signal Dev.clap on this machine");
        return None;
    };
    let daw = daw::standalone::Standalone::new();
    daw.set_plugin_load_options(daw::standalone::plugin::LoadOptions {
        actions: Some(Arc::new(StudioActions::default())),
        ..daw::standalone::plugin::LoadOptions::default()
    });
    let ctx = crate::clap_fx::test_support::drum_song(&daw, song);
    let track = crate::actions::resolve(&daw, &ctx, "Drums/Kick/Sum/In").expect("the track");
    let chain = daw::service::FxChainContext::Track(track);
    let fx = crate::clap_fx::add(&daw, &ctx, &chain, &clap, u32::MAX).expect("Signal Dev loads");
    if daw.with_plugin_instance(&fx, |p| p.signal().is_some()) != Some(true) {
        eprintln!("skipped: {} has no signal/1", clap.display());
        return None;
    }
    daw.with_plugin_instance(&fx, |p| {
        if !p.is_prepared() {
            p.prepare(48_000.0, 512).expect("activates");
        }
    });
    for (kind, after) in [("eq", None), ("comp", Some("tone.eq"))] {
        let mut args = serde_json::json!({ "phase": "tone", "kind": kind });
        if let Some(a) = after {
            args["after"] = a.into();
        }
        super::signal::run(
            &daw,
            &ctx,
            ("Kick/Sum/In", None),
            "SIGNAL_BLOCK_ADD",
            &args.to_string(),
        )
        .expect("a block added");
    }
    Some((daw, ctx, fx))
}

/// Parameter `id` of block `block` in a signal.chain/1.
fn chain_param(chain: &serde_json::Value, block: &str, id: &str) -> Option<serde_json::Value> {
    chain["phases"]
        .as_array()?
        .iter()
        .flat_map(|p| p["blocks"].as_array().cloned().unwrap_or_default())
        .find(|b| b["id"] == block)?["params"]
        .as_array()?
        .iter()
        .find(|p| p["id"] == id)
        .cloned()
}

fn value_of(chain: &serde_json::Value, block: &str, id: &str) -> f64 {
    chain_param(chain, block, id)
        .and_then(|p| p["value"].as_f64())
        .unwrap_or_else(|| panic!("no {block} {id} in {chain}"))
}

/// A render's copy of `fx` as LISTEN makes one (the live instance's
/// state into a new instance): its chain, and the gain in dB it gives a
/// -40 dBFS tone (under any compressor's threshold).
fn render_copy(
    daw: &daw::standalone::Standalone,
    ctx: &daw::service::ProjectContext,
    fx: &str,
    tone_hz: f32,
) -> (serde_json::Value, f64) {
    use daw_standalone::audio_engine::render::offline::{Copying, copy_plugin, hosted};
    let song = crate::fx_actions::project_guid(ctx).expect("a song");
    let (_, name) = hosted(daw, &song)
        .into_iter()
        .find(|(g, _)| g == fx)
        .expect("hosted");
    let state = daw.save_plugin_state(fx).expect("its state");
    let (mut copy, _) = copy_plugin(daw, &name, state, Copying::default()).expect("a copy");
    let chain: serde_json::Value = serde_json::from_str(
        &copy
            .signal()
            .expect("signal/1")
            .chain()
            .expect("the copy's chain"),
    )
    .expect("JSON");
    copy.prepare(48_000.0, 512).expect("the copy activates");
    let tone: Vec<f32> = (0..48_000)
        .map(|i| 0.01 * (std::f32::consts::TAU * tone_hz * i as f32 / 48_000.0).sin())
        .collect();
    let (mut l, mut r) = (vec![0.0; tone.len()], vec![0.0; tone.len()]);
    for at in (0..tone.len()).step_by(512) {
        let end = (at + 512).min(tone.len());
        copy.process_block(
            &tone[at..end],
            &tone[at..end],
            &mut l[at..end],
            &mut r[at..end],
            &daw_standalone::plugin::PluginEvents::EMPTY,
        )
        .expect("the copy plays");
    }
    // The second half: past any attack.
    let rms =
        |x: &[f32]| (x.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / x.len() as f64).sqrt();
    let half = tone.len() / 2;
    (chain, 20.0 * (rms(&l[half..]) / rms(&tone[half..])).log10())
}

/// FX_SET `@clap_id` to `text` on the Signal Dev on `Drums/Kick/Sum/In`.
fn fx_set(
    daw: &daw::standalone::Standalone,
    ctx: &daw::service::ProjectContext,
    clap_id: &str,
    text: &str,
) {
    let set = std::iter::once((format!("@{clap_id}"), text.to_owned())).collect();
    let said = crate::fx_actions::set(daw, ctx, ("Drums/Kick/Sum/In", "Signal Dev"), &set)
        .expect("FX_SET");
    assert!(said.iter().all(|s| s.contains(" = ")), "FX_SET: {said:?}");
}

/// FX_SET on a Signal instance that is not processing (activated, the
/// transport stopped) lands in Signal itself: its chain read and a render
/// copy made right after (from its state) both have the new value — and
/// the copy plays it.
#[test]
fn an_fx_set_reaches_a_stopped_signal_its_chain_and_a_render_copy() {
    let Some((daw, ctx, fx)) = stopped_signal("signal-fx-set-stopped") else {
        return;
    };
    let before = super::signal::chain(&daw, &ctx, "Kick/Sum/In", None).expect("its chain");
    assert!((value_of(&before["chain"], "tone.comp", "makeup")).abs() < 1e-6);
    let clap_id = chain_param(&before["chain"], "tone.comp", "makeup")
        .and_then(|p| p["clap_id"].as_str().map(str::to_owned))
        .expect("the comp's makeup is a slot");
    fx_set(&daw, &ctx, &clap_id, "6 dB");
    let after = super::signal::chain(&daw, &ctx, "Kick/Sum/In", None).expect("its chain");
    assert!(
        (value_of(&after["chain"], "tone.comp", "makeup") - 6.0).abs() < 1e-3,
        "the chain read has it: {}",
        chain_param(&after["chain"], "tone.comp", "makeup").unwrap()
    );
    let (copy, gain_db) = render_copy(&daw, &ctx, &fx, 220.0);
    assert!(
        (value_of(&copy, "tone.comp", "makeup") - 6.0).abs() < 1e-3,
        "the copy's state has it"
    );
    assert!(
        (gain_db - 6.0).abs() < 0.5,
        "the copy plays it: {gain_db:.2} dB"
    );
}

/// SIGNAL_FEED's `values_revision` is the chain's, before and after a value
/// set — read from SIGNAL_REVISIONS where the build has it (no chain
/// read), else from the chain — and a feed of a still song reads no
/// chain once it has one.
#[test]
fn the_feeds_values_revision_is_the_chains_and_follows_a_set() {
    let Some((daw, ctx, fx)) = stopped_signal("signal-feed-values") else {
        return;
    };
    let chain = |daw: &daw::standalone::Standalone| {
        super::signal::forget_chain(&fx);
        super::signal::chain(daw, &ctx, "Kick/Sum/In", None).expect("its chain")
    };
    let before = chain(&daw);
    let feed = super::signal::feed(&daw, &ctx);
    assert_eq!(feed.fx.len(), 1, "{feed:?}");
    assert_eq!(
        feed.fx[0].values_revision,
        before["chain"]["values_revision"].as_u64()
    );
    let clap_id = chain_param(&before["chain"], "tone.comp", "makeup")
        .and_then(|p| p["clap_id"].as_str().map(str::to_owned))
        .expect("the comp's makeup is a slot");
    fx_set(&daw, &ctx, &clap_id, "3 dB");
    let fed = super::signal::feed(&daw, &ctx).fx[0].values_revision;
    let after = chain(&daw);
    assert_eq!(fed, after["chain"]["values_revision"].as_u64());
    assert_ne!(fed, feed.fx[0].values_revision, "a value set moves it");
    // What a feed costs once the chain kept is older than VALUES_FRESH
    // (every other feed or so, at the mixer's ten a second).
    let at = std::time::Instant::now();
    for _ in 0..20 {
        super::signal::forget_chain(&fx);
        std::hint::black_box(super::signal::feed(&daw, &ctx));
    }
    eprintln!(
        "a feed of one Signal, its chain stale: {:?}",
        at.elapsed() / 20
    );
}

/// SIGNAL_BLOCK_SET on a stopped Signal: an EQ band past the six the slots
/// show, and a comp's makeup over a value FX_SET held on it — in the chain
/// read at once and in a render copy, which plays the makeup.
#[test]
fn a_signal_block_set_reaches_a_band_past_the_slots_and_a_comps_makeup() {
    let Some((daw, ctx, fx)) = stopped_signal("signal-block-set") else {
        return;
    };
    let listed = super::signal::run(&daw, &ctx, ("Kick/Sum/In", None), "list", "{}").unwrap();
    if !listed["result"].to_string().contains("SIGNAL_BLOCK_SET") {
        eprintln!("skipped: this Signal Dev has no SIGNAL_BLOCK_SET");
        return;
    }
    let set = super::signal::block_set(
        &daw,
        &ctx,
        &serde_json::json!({
            "track": "Kick/Sum/In",
            "block": "tone.eq",
            "params": { "band.7.used": true, "band.7.freq": 450, "band.7.gain": "-6 dB", "band.7.shape": "Bell" },
        }),
    )
    .expect("the EQ set");
    assert_eq!(set["block"]["id"], "tone.eq", "{set}");
    let band = |chain: &serde_json::Value| {
        (
            value_of(chain, "tone.eq", "band.7.freq"),
            value_of(chain, "tone.eq", "band.7.gain"),
        )
    };
    let read = super::signal::chain(&daw, &ctx, "Kick/Sum/In", None).unwrap();
    assert_eq!(band(&read["chain"]), (450.0, -6.0));
    assert!(
        chain_param(&read["chain"], "tone.eq", "band.7.freq").unwrap()["clap_id"].is_null(),
        "band 7 is past the slots"
    );

    // A value FX_SET holds on the makeup (sent with every block played) is
    // let go: the block's own value is the one heard.
    let clap_id = chain_param(&read["chain"], "tone.comp", "makeup").unwrap()["clap_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let index = crate::fx_actions::slot_named(&daw, &fx, &format!("@{clap_id}"))
        .unwrap()
        .unwrap();
    fx_set(&daw, &ctx, &clap_id, "3 dB");
    let set = super::signal::block_set(
        &daw,
        &ctx,
        &serde_json::json!({ "track": "Kick/Sum/In", "block": "comp", "params": { "makeup": 9 } }),
    )
    .expect("the makeup set");
    assert_eq!(set["let_go"], 1, "{set}");
    let read = super::signal::chain(&daw, &ctx, "Kick/Sum/In", None).unwrap();
    assert!((value_of(&read["chain"], "tone.comp", "makeup") - 9.0).abs() < 1e-3);
    let held = daw::service::Effects::parameter(
        &daw,
        ctx.clone(),
        daw::service::FxTarget {
            context: daw::service::FxChainContext::Track(
                crate::actions::resolve(&daw, &ctx, "Drums/Kick/Sum/In").unwrap(),
            ),
            fx: daw::service::FxRef::Guid(fx.clone()),
        },
        index,
    )
    .unwrap();
    assert!(
        (held.value - 9.0 / 24.0).abs() < 1e-3,
        "FX_PARAMS reads it: {held:?}"
    );

    let (copy, gain_db) = render_copy(&daw, &ctx, &fx, 450.0);
    assert_eq!(band(&copy), (450.0, -6.0), "the copy has band 7");
    assert!((value_of(&copy, "tone.comp", "makeup") - 9.0).abs() < 1e-3);
    // A 450 Hz tone: band 7's -6 dB at its centre, then the 9 dB makeup.
    assert!(
        (gain_db - 3.0).abs() < 0.5,
        "the copy plays both: {gain_db:.2} dB"
    );
}

/// The comp's makeup set through the state (FX_STATE, the preset edited,
/// FX_STATE_SET — what agents did before SIGNAL_BLOCK_SET): the chain read
/// and a render copy have it.
#[test]
fn a_comps_makeup_set_through_the_state_holds() {
    let Some((daw, ctx, fx)) = stopped_signal("signal-makeup-state") else {
        return;
    };
    let read = super::signal::chain(&daw, &ctx, "Kick/Sum/In", None).unwrap();
    let clap_id = chain_param(&read["chain"], "tone.comp", "makeup").unwrap()["clap_id"]
        .as_str()
        .unwrap()
        .to_owned();
    // A value FX_SET held before.
    fx_set(&daw, &ctx, &clap_id, "3 dB");
    let saved = daw.save_plugin_state(&fx).unwrap();
    let len = u64::from_le_bytes(saved[..8].try_into().unwrap()) as usize;
    let mut outer: serde_json::Value = serde_json::from_slice(&saved[8..8 + len]).unwrap();
    let text: String =
        serde_json::from_str(outer["fields"]["signal-state"].as_str().unwrap()).unwrap();
    let mut state: serde_json::Value = serde_json::from_str(&text).unwrap();
    let comp = state["preset"]["lanes"][0]["blocks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["id"] == "comp")
        .unwrap();
    comp["params"]["makeup"] = 12.0.into();
    outer["fields"]["signal-state"] = serde_json::to_string(&state.to_string()).unwrap().into();
    let body = serde_json::to_vec(&outer).unwrap();
    let mut blob = (body.len() as u64).to_le_bytes().to_vec();
    blob.extend_from_slice(&body);
    let song = crate::fx_actions::project_guid(&ctx).unwrap();
    crate::clap_fx::load_state(&daw, &song, &fx, &blob).expect("FX_STATE_SET");
    let read = super::signal::chain(&daw, &ctx, "Kick/Sum/In", None).unwrap();
    assert!(
        (value_of(&read["chain"], "tone.comp", "makeup") - 12.0).abs() < 1e-3,
        "the chain read: {}",
        chain_param(&read["chain"], "tone.comp", "makeup").unwrap()
    );
    let (copy, gain_db) = render_copy(&daw, &ctx, &fx, 220.0);
    assert!(
        (value_of(&copy, "tone.comp", "makeup") - 12.0).abs() < 1e-3,
        "the copy's state"
    );
    assert!(
        (gain_db - 12.0).abs() < 0.5,
        "the copy plays it: {gain_db:.2} dB"
    );
}
