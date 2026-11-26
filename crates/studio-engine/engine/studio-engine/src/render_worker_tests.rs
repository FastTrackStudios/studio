//! A real render worker (this test binary, in worker mode) rendering a
//! test song the same as the engine does, to the bit, surviving a plugin
//! crash that the engine never sees. Moved here from engine-listen's
//! `render_worker` tests: the worker serves the engine's whole Action List
//! (`crate::install`), which only the assembly has.

use std::path::{Path, PathBuf};
use web_time::Instant;

use architect::action::{ActionCaller, Invocation, InvocationSource, Principal};
use serde_json::{Value, json};

use crate::render_worker::client::{Failure, Spawner};

// --- A real worker -------------------------------------------------------

/// This test binary, in worker mode: what the engine spawns as its worker
/// in these tests. Ignored when run on its own.
#[test]
#[ignore = "the render worker's entry, run by the tests below"]
fn worker_entry() {
    if let Some(result) = crate::render_worker::main_if_worker(crate::install) {
        result.expect("the worker ran");
        std::process::exit(0);
    }
}

fn this_binary_as_worker() -> Spawner {
    Spawner {
        program: std::env::current_exe().expect("this test binary"),
        args: [
            "render_worker_tests::worker_entry",
            "--exact",
            "--include-ignored",
            "--nocapture",
            "--test-threads=1",
        ]
        .map(str::to_owned)
        .to_vec(),
        env: vec![("FTS_ENGINE_LOG".into(), "warn".into())],
    }
}

/// A mono 32-bit float WAV of `samples` at 48 kHz.
fn write_wav(path: &Path, samples: &[f32]) {
    let data = u32::try_from(samples.len() * 4).expect("small");
    let mut out = Vec::with_capacity(44 + samples.len() * 4);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&48_000u32.to_le_bytes());
    out.extend_from_slice(&(48_000u32 * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, out).expect("a wav");
}

/// Hits: a decaying 180 Hz thump every half second, `seconds` long.
fn thumps(seconds: f64, hz: f64, every: f64) -> Vec<f32> {
    let n = (seconds * 48_000.0) as usize;
    (0..n)
        .map(|i| {
            let t = i as f64 / 48_000.0;
            let since = t % every;
            (0.8 * (-since * 18.0).exp() * (std::f64::consts::TAU * hz * since).sin()) as f32
        })
        .collect()
}

async fn run(id: &str, args: Value) -> Value {
    let out = crate::actions::list()
        .run_json_async(id, args.to_string().as_bytes())
        .await
        .unwrap_or_else(|e| panic!("{id}: {e}"));
    serde_json::from_slice(&out).expect("json")
}

/// The test song: Drums{Kick, Snare} from WAVs on disk, the built-in
/// `FTS Comp` on Kick, saved as a project file and opened from it, as a
/// person's song is. Its folder and the open song's guid.
fn open_test_song(dir: &Path, rt: &tokio::runtime::Runtime) -> (PathBuf, String) {
    use daw_standalone::media_seed::{StemSpec, seed_media_tracks};
    let audio = dir.join("Audio");
    std::fs::create_dir_all(&audio).expect("a folder");
    write_wav(&audio.join("kick.wav"), &thumps(6.0, 60.0, 0.5));
    write_wav(&audio.join("snare.wav"), &thumps(6.0, 200.0, 1.0));
    let daw = crate::open::start_engine().expect("an engine");
    let guid = uuid::Uuid::new_v4().to_string();
    let file = dir.join("Worker Song.session");
    daw.seed_project(daw_proto::ProjectInfo {
        guid: guid.clone(),
        name: "Worker Song".into(),
        path: file.to_string_lossy().into_owned(),
    });
    let stems = [
        StemSpec::new(
            "Kick",
            audio.join("kick.wav").to_string_lossy(),
            Some("Drums"),
        ),
        StemSpec::new(
            "Snare",
            audio.join("snare.wav").to_string_lossy(),
            Some("Drums"),
        ),
    ];
    seed_media_tracks(&daw, &guid, &stems, 0.0, 6.0);
    crate::open::switch_to(&daw, &guid, false);
    rt.block_on(run("FX_ADD", json!({ "track": "Kick", "fx": "FTS Comp" })));
    crate::project_file::save(&daw, &guid, &file, session_format::store::MAIN, "Save")
        .expect("saved");
    let opened = crate::open::open_silent(&file).expect("opened from its file");
    (dir.to_path_buf(), opened.project_guid)
}

/// `answer` without what differs between two runs of the same render:
/// timings, where pictures went, how it was answered.
fn figures(answer: &Value) -> Value {
    const TIMES: &[&str] = &[
        "ms",
        "analysis_ms",
        "copy_ms",
        "prime_ms",
        "blocks_ms",
        "rendered",
        "png",
        "waveform",
        "spectrogram",
        "realtime_factor",
        "answered",
        "file",
        "render_ms",
    ];
    match answer {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| !TIMES.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), figures(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(figures).collect()),
        other => other.clone(),
    }
}

fn timed(rt: &tokio::runtime::Runtime, id: &str, args: Value) -> (Value, f64) {
    let began = Instant::now();
    let answer = rt.block_on(run(id, args));
    (answer, began.elapsed().as_secs_f64() * 1e3)
}

fn with(args: &Value, place: &str) -> Value {
    let mut a = args.clone();
    a["_render_in"] = json!(place);
    a
}

#[test]
fn the_worker_renders_what_the_engine_renders_and_survives_a_plugin_crash() {
    // SAFETY: before this test starts any thread of its own; nextest runs
    // each test in a process of its own.
    unsafe {
        std::env::set_var("FTS_ENGINE_SILENT", "1");
        std::env::set_var("FTS_MCP", "0");
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("a runtime");
    let dir = tempfile::tempdir().expect("a folder");
    let (_, live) = open_test_song(dir.path(), &rt);
    crate::render_worker::client::global()
        .lock()
        .expect("the worker")
        .configure(this_binary_as_worker(), true);

    let listen =
        json!({ "track": "Drums", "from": "0.5s", "to": "4s", "exact": true, "png": false });
    let measure = json!({ "from": "0.5s", "to": "4s", "exact": true });
    let mut numbers = Vec::new();

    // In process: first (cold copies), then warm.
    let (p1, ms) = timed(&rt, "LISTEN", with(&listen, "process"));
    numbers.push(("LISTEN in process, first", ms));
    let (_, ms) = timed(&rt, "LISTEN", with(&listen, "process"));
    numbers.push(("LISTEN in process, warm", ms));
    // In the worker: first (spawn, snapshot, cold copies), then warm.
    let (w1, ms) = timed(&rt, "LISTEN", with(&listen, "worker"));
    numbers.push(("LISTEN in the worker, first (spawn + sync)", ms));
    let (w2, ms) = timed(&rt, "LISTEN", with(&listen, "worker"));
    numbers.push(("LISTEN in the worker, warm", ms));
    assert!(p1["figures"]["lufs"].is_number(), "{p1}");
    assert_eq!(figures(&w1), figures(&p1), "the worker's first LISTEN");
    assert_eq!(figures(&w2), figures(&p1), "the worker's warm LISTEN");

    let (pm, ms) = timed(&rt, "MIX_MEASURE", with(&measure, "process"));
    numbers.push(("MIX_MEASURE in process", ms));
    let (wm, ms) = timed(&rt, "MIX_MEASURE", with(&measure, "worker"));
    numbers.push(("MIX_MEASURE in the worker", ms));
    assert_eq!(figures(&wm), figures(&pm), "MIX_MEASURE");

    // An edit in the live song reaches the worker as an update.
    rt.block_on(run(
        "TRACK_SET",
        json!({ "track": "Kick", "volume_db": -6.0 }),
    ));
    let (pe, _) = timed(&rt, "LISTEN", with(&listen, "process"));
    let (we, ms) = timed(&rt, "LISTEN", with(&listen, "worker"));
    numbers.push(("LISTEN in the worker after an edit (update sync)", ms));
    assert_ne!(figures(&pe), figures(&p1), "the edit changed what is heard");
    assert_eq!(figures(&we), figures(&pe), "the worker heard the edit");
    assert_eq!(
        crate::render_worker::client::status().song.as_deref(),
        Some(live.as_str())
    );

    // A plugin crashes in the worker: the call says so and names it, this
    // process (the live session) goes on, and a fresh worker renders.
    let before = crate::render_worker::client::status();
    let crashed = {
        let song = crate::render_worker::LiveSong::current().expect("the song");
        crate::render_worker::client::global()
            .lock()
            .expect("the worker")
            .fault(&song, "segv", "Kick: Crashy (test)")
    };
    match crashed {
        Err(Failure::PluginCrashed { plugin, signal, .. }) => {
            assert_eq!(
                plugin.map(|p| p.name).as_deref(),
                Some("Kick: Crashy (test)")
            );
            assert_eq!(signal.as_deref(), Some("SIGSEGV"));
        }
        other => panic!("not a crash: {other:?}"),
    }
    let after = crate::render_worker::client::status();
    assert_eq!(after.restarts, before.restarts + 1);
    assert!(
        after
            .last_crash
            .as_deref()
            .is_some_and(|m| m.starts_with("plugin_crashed:"))
    );
    assert_ne!(after.pid, before.pid, "a fresh worker started behind it");
    let (pl, _) = timed(&rt, "LISTEN", with(&listen, "process"));
    assert_eq!(figures(&pl), figures(&pe), "the live song is untouched");
    let (wr, ms) = timed(&rt, "LISTEN", with(&listen, "worker"));
    numbers.push(("LISTEN in the worker after a crash (re-sync)", ms));
    assert_eq!(
        figures(&wr),
        figures(&pe),
        "the fresh worker renders the same"
    );

    // The numbers, for the report (`--nocapture`).
    let table: Vec<Value> = numbers
        .iter()
        .map(|(what, ms)| json!({ "what": what, "ms": (ms * 10.0).round() / 10.0 }))
        .collect();
    let out = std::env::temp_dir().join("render-worker-overhead.json");
    std::fs::write(
        &out,
        serde_json::to_string_pretty(&table).unwrap_or_default(),
    )
    .ok();
    crate::render_worker::client::global()
        .lock()
        .expect("the worker")
        .stop();
}
