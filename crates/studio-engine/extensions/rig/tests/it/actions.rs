//! Every `STUDIO_RIG_*` action the rig engine declares, run against the
//! engine in design mode (no audio device, no MIDI) on a library in a temp
//! folder, each with valid and invalid arguments: the answer's shape, and
//! the rig's state after (studio#560). What "switch" means in studio#670:
//! a mode, a song, a section, a stack, a patch, a footswitch, a macro, a
//! block, each moved by action and read back.
//!
//! One test: the engine is a process-wide one (`rig_extension::start_engine`),
//! and nextest runs each test in a process of its own.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use architect::action::ActionList;
use architect_extensions::extensions::Extensions;
use architect_extensions::host::{Host, SharedSections};
use architect_extensions::install::Installs;
use architect_settings::Registry;
use serde_json::{Value, json};

/// The engine, linked into a host as Studio links an extension, and what
/// the test ran on it.
struct Rig {
    rt: tokio::runtime::Runtime,
    list: ActionList,
    ran: RefCell<BTreeSet<String>>,
    _ext: Extensions,
    _home: tempfile::TempDir,
}

impl Rig {
    /// The engine in design mode, everything it reads and writes in a temp
    /// home, its actions on a host's list; once its rigs are up.
    fn start() -> Self {
        let home = tempfile::tempdir().unwrap();
        let at = |p: &str| home.path().join(p);
        // SAFETY: set before any thread of this process exists that reads
        // them (the engine's thread starts below); nextest runs this test
        // alone in its process.
        unsafe {
            std::env::set_var("HOME", home.path());
            std::env::set_var("XDG_CONFIG_HOME", at("config"));
            std::env::set_var("XDG_CACHE_HOME", at("cache"));
            std::env::set_var("XDG_RUNTIME_DIR", at("run"));
            std::env::set_var("SIGNAL_RIG_DESIGN", "1");
            std::env::set_var("SIGNAL_RIG_DIR", at("rig"));
            std::env::set_var("SIGNAL_LIBRARIES_DIR", at("libraries"));
            std::env::remove_var("STUDIO_RIG_AUTOSTART");
            std::env::remove_var("STUDIO_RIG_URL");
        }
        let list = ActionList::new();
        let host = Host::new(list.clone(), SharedSections::new(Registry::new()));
        let ext = Extensions::new(host, Installs::new(at("installs")))
            .with_linked(&rig_extension::RigEngine)
            .unwrap();
        let _ = ext.sync();
        rig_extension::start_engine();
        let rig = Self {
            rt: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
            list,
            ran: RefCell::new(BTreeSet::new()),
            _ext: ext,
            _home: home,
        };
        let began = Instant::now();
        loop {
            let state = rig.ok("STUDIO_RIG_STATE", json!({}));
            if state["up"] == true {
                break;
            }
            assert!(
                began.elapsed() < Duration::from_secs(120),
                "the rig engine did not come up: {state}"
            );
            std::thread::sleep(Duration::from_millis(200));
        }
        rig
    }

    fn call(&self, id: &str, args: &Value) -> Result<Value, String> {
        self.ran.borrow_mut().insert(id.to_owned());
        let out = self
            .rt
            .block_on(self.list.run_json_async(id, args.to_string().as_bytes()))
            .map_err(|e| e.to_string())?;
        Ok(serde_json::from_slice(&out)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&out).into_owned())))
    }

    /// `id`'s answer to `args`, which it must accept.
    fn ok(&self, id: &str, args: Value) -> Value {
        match self.call(id, &args) {
            Ok(v) => v,
            Err(e) => panic!("{id} {args} was refused: {e}"),
        }
    }

    /// Why `id` refuses `args`, which it must; the reason names `says`.
    fn refused(&self, id: &str, args: Value, says: &str) {
        match self.call(id, &args) {
            Ok(v) => panic!("{id} {args} was accepted: {v}"),
            Err(e) => assert!(e.contains(says), "{id} {args}: {e:?} does not say {says:?}"),
        }
    }

    /// Where the rig is now (`STUDIO_RIG_STATE`'s `at`).
    fn at(&self) -> Value {
        self.ok("STUDIO_RIG_STATE", json!({}))["at"].clone()
    }

    fn perf(&self) -> Value {
        self.ok("STUDIO_RIG_PERFORMANCE", json!({}))
    }
}

/// How many items `v` (a list) has.
fn count(v: &Value) -> u64 {
    v.as_array()
        .map_or(0, |a| u64::try_from(a.len()).unwrap_or(u64::MAX))
}

/// Every `STUDIO_RIG_*` action the engine's manifest declares.
fn declared() -> BTreeSet<String> {
    rig_extension::MANIFEST
        .lines()
        .filter_map(|l| l.trim().strip_prefix("id = \""))
        .filter_map(|l| l.strip_suffix('"'))
        .filter(|id| id.starts_with("STUDIO_RIG_"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_rig_action_answers_and_moves_the_rig_in_design_mode() {
    let rig = Rig::start();

    // ── The engine ────────────────────────────────────────────────────
    let state = rig.ok("STUDIO_RIG_STATE", json!({}));
    assert_eq!(state["up"], true, "{state}");
    assert_eq!(state["design"], true, "{state}");
    assert_eq!(state["instrument"], "guitar", "{state}");
    // Audio closed until asked for (studio#555): no STUDIO_RIG_AUTOSTART.
    assert_eq!(state["running"], false, "{state}");
    for key in ["url", "library", "at", "status"] {
        assert!(
            !state[key].is_null(),
            "STUDIO_RIG_STATE has no {key}: {state}"
        );
    }
    for id in ["STUDIO_RIG_START", "STUDIO_RIG_RESTART"] {
        let s = rig.ok(id, json!({}));
        assert!(s["running"].is_boolean(), "{id}: {s}");
    }
    let stopped = rig.ok("STUDIO_RIG_STOP", json!({}));
    assert_eq!(stopped["running"], false, "{stopped}");
    assert_eq!(rig.ok("STUDIO_RIG_PANIC", json!({}))["ok"], true);
    assert_eq!(rig.ok("STUDIO_RIG_TAP", json!({}))["ok"], true);

    // ── Mute: named, read back, and the bar's flip ───────────────────
    let both = rig.ok("STUDIO_RIG_MUTE", json!({ "house": true, "phones": true }));
    assert_eq!(both, json!({ "house": true, "phones": true }));
    let phones = &rig.perf()["headphone"];
    assert_eq!(phones["main_mute"], true, "{phones}");
    assert_eq!(phones["phones_mute"], true, "{phones}");
    let flipped = rig.ok("STUDIO_RIG_MUTE", json!({}));
    assert_eq!(
        flipped["house"], false,
        "the bare call flips the house: {flipped}"
    );
    rig.ok(
        "STUDIO_RIG_MUTE",
        json!({ "house": false, "phones": false }),
    );

    // ── The footswitch mode ──────────────────────────────────────────
    for (asked, mode) in [(json!("setlist"), 2), (json!(0), 0), (json!("profile"), 1)] {
        let at = rig.ok("STUDIO_RIG_MODE", json!({ "mode": asked }));
        assert_eq!(at["mode"], mode, "MODE {asked}: {at}");
        assert_eq!(rig.at()["mode"], mode, "read back after MODE {asked}");
    }
    rig.refused("STUDIO_RIG_MODE", json!({ "mode": "loud" }), "no mode loud");
    rig.refused("STUDIO_RIG_MODE", json!({ "mode": 7 }), "mode is 0");
    rig.refused("STUDIO_RIG_MODE", json!({}), "no mode");

    // ── Songs and sections, in the setlist mode ──────────────────────
    rig.ok("STUDIO_RIG_MODE", json!({ "mode": "setlist" }));
    let songs = count(&rig.perf()["songs"]);
    rig.refused("STUDIO_RIG_SONG", json!({}), "no song");
    rig.refused("STUDIO_RIG_SONG", json!({ "song": 0 }), "counts from 1");
    rig.refused(
        "STUDIO_RIG_SONG",
        json!({ "name": "no such song" }),
        "no song named",
    );
    // A number past the set is refused, never the last song instead.
    rig.refused("STUDIO_RIG_SONG", json!({ "song": songs + 1 }), "there are");
    if songs > 0 {
        let at = rig.ok("STUDIO_RIG_SONG", json!({ "song": songs }));
        assert_eq!(at["song"]["number"], songs, "{at}");
        let first = rig.perf()["songs"][0]["name"].clone();
        let at = rig.ok("STUDIO_RIG_SONG", json!({ "name": first }));
        assert_eq!(at["song"]["number"], 1, "SONG by name {first}: {at}");
        if songs > 1 {
            let at = rig.ok("STUDIO_RIG_SONG", json!({ "step": 1 }));
            assert_eq!(at["song"]["number"], 2, "{at}");
            let at = rig.ok("STUDIO_RIG_SONG", json!({ "step": -1 }));
            assert_eq!(at["song"]["number"], 1, "{at}");
        }
    }
    let parts = count(&rig.perf()["parts"]);
    rig.refused("STUDIO_RIG_SECTION", json!({}), "no section");
    rig.refused(
        "STUDIO_RIG_SECTION",
        json!({ "section": 0 }),
        "counts from 1",
    );
    rig.refused(
        "STUDIO_RIG_SECTION",
        json!({ "section": parts + 1 }),
        "there are",
    );
    rig.refused(
        "STUDIO_RIG_SECTION",
        json!({ "name": "no such section" }),
        "no section named",
    );
    if parts > 0 {
        let at = rig.ok("STUDIO_RIG_SECTION", json!({ "section": parts }));
        assert_eq!(at["section"]["number"], parts, "{at}");
        rig.ok("STUDIO_RIG_SECTION", json!({ "step": -1 }));
    }
    rig.ok("STUDIO_RIG_SETLIST", json!({ "setlist": 1 }));
    rig.refused(
        "STUDIO_RIG_SETLIST",
        json!({ "setlist": 0 }),
        "counts from 1",
    );
    rig.refused("STUDIO_RIG_SETLIST", json!({}), "no setlist");

    // ── Profiles, stacks, patches, presets, footswitches ─────────────
    let library = rig.ok("STUDIO_RIG_LIBRARY", json!({}));
    assert!(library["profiles"].is_array(), "{library}");
    if let Some(profile) = library["profiles"][0]["name"].as_str() {
        rig.ok("STUDIO_RIG_PROFILE", json!({ "name": profile }));
    }
    rig.refused("STUDIO_RIG_PROFILE", json!({}), "no name");

    let stacks = count(&rig.perf()["stacks"]);
    rig.refused("STUDIO_RIG_STACK", json!({}), "no stack");
    rig.refused(
        "STUDIO_RIG_STACK",
        json!({ "stack": stacks + 1 }),
        "there are",
    );
    rig.refused(
        "STUDIO_RIG_STACK",
        json!({ "name": "no such stack" }),
        "no stack named",
    );
    if stacks > 0 {
        let at = rig.ok("STUDIO_RIG_STACK", json!({ "stack": 1 }));
        assert!(
            at.get("mode").is_some(),
            "STACK answers where the rig is: {at}"
        );
    }

    let patches = rig.ok("STUDIO_RIG_PATCHES", json!({}));
    let n = count(&patches);
    rig.refused("STUDIO_RIG_PATCH", json!({}), "no patch");
    rig.refused("STUDIO_RIG_PATCH", json!({ "patch": n + 1 }), "there are");
    rig.refused(
        "STUDIO_RIG_PATCH",
        json!({ "name": "no such patch" }),
        "no patch named",
    );
    if n > 0 {
        rig.ok("STUDIO_RIG_PATCH", json!({ "patch": 1 }));
        rig.ok("STUDIO_RIG_PATCH", json!({ "name": patches[0]["name"] }));
    }

    let presets = rig.ok("STUDIO_RIG_PRESETS", json!({}));
    assert!(presets.is_array(), "{presets}");
    rig.refused("STUDIO_RIG_PRESET", json!({}), "no preset");
    rig.refused(
        "STUDIO_RIG_PRESET",
        json!({ "number": count(&presets) + 1 }),
        "there are",
    );
    if count(&presets) > 0 {
        rig.ok("STUDIO_RIG_PRESET", json!({ "number": 1 }));
    }

    for args in [json!({ "switch": 1 }), json!({ "switch": 2, "hold": true })] {
        let at = rig.ok("STUDIO_RIG_SWITCH", args.clone());
        assert!(
            at.get("song").is_some(),
            "SWITCH {args} answers where the rig is: {at}"
        );
    }
    rig.refused("STUDIO_RIG_SWITCH", json!({}), "no switch");
    rig.refused("STUDIO_RIG_SWITCH", json!({ "switch": 0 }), "counts from 1");

    // ── The sound: macros and blocks ─────────────────────────────────
    let macros = rig.ok("STUDIO_RIG_MACROS", json!({}));
    assert!(macros.is_array(), "{macros}");
    let first_macro = macros
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|m| m["children"].as_array().cloned().unwrap_or_default())
        .find_map(|c| c["id"].as_str().map(str::to_owned));
    rig.refused("STUDIO_RIG_MACRO", json!({}), "no id");
    if let Some(id) = first_macro {
        rig.refused("STUDIO_RIG_MACRO", json!({ "id": id }), "no value");
        let set = rig.ok("STUDIO_RIG_MACRO", json!({ "id": id, "value": 0.8 }));
        assert_eq!(set["ok"], true, "{set}");
        // Read back as set (studio#560 doubted 0.8 read 68 %).
        let now = rig
            .ok("STUDIO_RIG_MACROS", json!({}))
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|m| m["children"].as_array().cloned().unwrap_or_default())
            .find(|c| c["id"] == id.as_str())
            .and_then(|c| c["value"].as_f64())
            .unwrap();
        assert!((now - 0.8).abs() < 1e-3, "macro {id} reads {now} after 0.8");
        assert_eq!(
            rig.ok("STUDIO_RIG_MACRO", json!({ "id": id, "reset": true }))["ok"],
            true
        );
    }

    let blocks = rig.ok("STUDIO_RIG_BLOCKS", json!({}));
    assert!(blocks.is_array(), "{blocks}");
    rig.refused("STUDIO_RIG_BLOCK", json!({}), "no id");
    rig.refused(
        "STUDIO_RIG_BLOCK",
        json!({ "id": "no-such-block" }),
        "no block",
    );
    if let Some(id) = blocks[0]["id"].as_str() {
        let block = rig.ok("STUDIO_RIG_BLOCK", json!({ "id": id, "bypass": true }));
        assert_eq!(block["id"], id, "{block}");
        assert!(block["bypassed"].is_boolean(), "{block}");
        rig.refused(
            "STUDIO_RIG_BLOCK",
            json!({ "id": id, "param": "mix" }),
            "needs a value",
        );
        rig.ok("STUDIO_RIG_BLOCK", json!({ "id": id, "bypass": false }));
    }
    let nodes = rig.ok("STUDIO_RIG_NODES", json!({}));
    assert!(nodes.is_array(), "{nodes}");
    rig.refused("STUDIO_RIG_BLOCK_PRESET", json!({}), "no preset");
    rig.refused(
        "STUDIO_RIG_BLOCK_PRESET",
        json!({ "preset": "x" }),
        "no block or module",
    );

    let tuner = rig.ok("STUDIO_RIG_TUNER", json!({ "toggle": true }));
    assert!(tuner.get("note").is_some(), "{tuner}");
    rig.ok("STUDIO_RIG_TUNER", json!({ "toggle": true }));

    // ── Setup: the instrument, the setup, audio, MIDI ────────────────
    // The one playing already is nothing to do (the Rig window asks for it).
    let same = rig.ok("STUDIO_RIG_INSTRUMENT", json!({ "instrument": "guitar" }));
    assert_eq!(same["instrument"], "guitar", "{same}");
    rig.refused(
        "STUDIO_RIG_INSTRUMENT",
        json!({ "instrument": "tuba" }),
        "no instrument tuba",
    );
    let bass = rig.ok("STUDIO_RIG_INSTRUMENT", json!({ "instrument": "bass" }));
    assert_eq!(bass["instrument"], "bass", "{bass}");
    let back = rig.ok("STUDIO_RIG_INSTRUMENT", json!({ "instrument": "guitar" }));
    assert_eq!(back["instrument"], "guitar", "{back}");

    // A setup and audio prefs saved as read: the JSON goes back into the
    // rig's own types (`unwire`) and answers the same.
    let setup = rig.ok("STUDIO_RIG_SETUP", json!({}));
    assert!(setup.is_object(), "{setup}");
    assert_eq!(
        rig.ok("STUDIO_RIG_SETUP", json!({ "setup": setup.clone() })),
        setup
    );
    let audio = rig.ok("STUDIO_RIG_AUDIO", json!({}));
    for key in ["prefs", "devices", "running"] {
        assert!(
            audio.get(key).is_some(),
            "STUDIO_RIG_AUDIO has no {key}: {audio}"
        );
    }
    let saved = rig.ok(
        "STUDIO_RIG_AUDIO",
        json!({ "prefs": audio["prefs"].clone(), "restart": false }),
    );
    assert_eq!(saved["prefs"], audio["prefs"]);
    rig.refused("STUDIO_RIG_AUDIO", json!({ "prefs": 7 }), "");

    let learning = rig.ok("STUDIO_RIG_MIDI", json!({ "learn": "tap" }));
    assert_eq!(learning["learn"]["learning"], "tap", "{learning}");
    let cancelled = rig.ok("STUDIO_RIG_MIDI", json!({ "cancel": true }));
    assert!(cancelled["learn"]["learning"].is_null(), "{cancelled}");
    assert!(rig.ok("STUDIO_RIG_MIDI", json!({ "unlearn": "tap" }))["ports"].is_array());

    // ── Signal's command list ────────────────────────────────────────
    rig.refused("STUDIO_RIG_RUN", json!({}), "no id");
    rig.refused(
        "STUDIO_RIG_RUN",
        json!({ "id": "FTS_SIGNAL_NOPE" }),
        "FTS_SIGNAL_NOPE",
    );
    rig.ok("STUDIO_RIG_RUN", json!({ "id": "FTS_SIGNAL_NEXT_SONG" }));

    // ── Offline render: every patch built and played a DI, no device ──
    let render = rig.ok("STUDIO_RIG_RENDER", json!({}));
    let (rendered, of) = (render["rendered"].as_u64(), render["of"].as_u64());
    assert!(
        matches!((rendered, of), (Some(r), Some(o)) if r <= o),
        "{render}"
    );
    assert_eq!(render["sample_rate"], 48_000, "{render}");

    // ── The keys rig ─────────────────────────────────────────────────
    let keys = rig.ok("STUDIO_RIG_KEYS_STATE", json!({}));
    assert!(
        keys["status"].is_object() && keys["perform"].is_object(),
        "{keys}"
    );
    assert!(rig.ok("STUDIO_RIG_KEYS_START", json!({})).is_object());
    let note = rig.ok("STUDIO_RIG_KEYS_NOTE", json!({ "note": 60 }));
    assert_eq!(note, json!({ "note": 60, "velocity": 100 }));
    let off = rig.ok("STUDIO_RIG_KEYS_NOTE", json!({ "note": 60, "velocity": 0 }));
    assert_eq!(off["velocity"], 0, "{off}");
    rig.refused("STUDIO_RIG_KEYS_NOTE", json!({ "note": 200 }), "0 to 127");
    rig.refused("STUDIO_RIG_KEYS_NOTE", json!({}), "no note");

    let key_presets = rig.ok("STUDIO_RIG_KEYS_PRESETS", json!({}));
    assert!(key_presets.is_array(), "{key_presets}");
    rig.refused("STUDIO_RIG_KEYS_PRESET", json!({}), "no preset");
    rig.refused(
        "STUDIO_RIG_KEYS_PRESET",
        json!({ "number": count(&key_presets) + 1 }),
        "there are",
    );
    if count(&key_presets) > 0 {
        rig.ok("STUDIO_RIG_KEYS_PRESET", json!({ "number": 1 }));
    }

    let key_stacks = count(&rig.ok("STUDIO_RIG_KEYS_STATE", json!({}))["perform"]["stacks"]);
    rig.refused("STUDIO_RIG_KEYS_STACK", json!({}), "no stack");
    rig.refused(
        "STUDIO_RIG_KEYS_STACK",
        json!({ "stack": key_stacks + 1 }),
        "there are",
    );
    rig.refused(
        "STUDIO_RIG_KEYS_STACK",
        json!({ "name": "no such stack" }),
        "no keys stack named",
    );
    if key_stacks > 0 {
        let perform = rig.ok("STUDIO_RIG_KEYS_STACK", json!({ "stack": 1 }));
        assert!(perform["stacks"].is_array(), "{perform}");
    }

    assert!(
        rig.ok("STUDIO_RIG_KEYS_MIXER", json!({ "db": -3.0 }))
            .is_object()
    );
    let key_macros = rig.ok("STUDIO_RIG_KEYS_MACROS", json!({}));
    assert!(key_macros.is_array(), "{key_macros}");
    rig.refused("STUDIO_RIG_KEYS_MACRO", json!({}), "no id");
    rig.refused("STUDIO_RIG_KEYS_MACRO", json!({ "id": "x" }), "no value");
    if let Some(id) = key_macros[0]["id"].as_str() {
        rig.ok("STUDIO_RIG_KEYS_MACRO", json!({ "id": id, "value": 0.5 }));
    }
    let midi = rig.ok("STUDIO_RIG_KEYS_MIDI", json!({ "cancel": true }));
    assert!(midi["ports"].is_array(), "{midi}");
    assert!(rig.ok("STUDIO_RIG_KEYS_STOP", json!({})).is_object());

    // ── Nothing declared and left unrun ──────────────────────────────
    let unrun: Vec<String> = declared().difference(&rig.ran.borrow()).cloned().collect();
    assert!(unrun.is_empty(), "declared but never run here: {unrun:?}");
}
