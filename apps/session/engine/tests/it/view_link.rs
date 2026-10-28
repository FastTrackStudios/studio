//! A window's model on an engine, through the window's client alone: the
//! engine stood up headless, a window linked to it over the in-memory vox
//! link (`open::link_window_here` — the link iOS and the browser use, the
//! same client as a desktop window's socket), and what the window draws
//! checked as it arrives over that link — the set (opened, then closed),
//! the waveforms as the engine draws them, the automation lanes and a take
//! volume curve the window's own edits wrote, and the hit overlay — and
//! the window's controls (the palette, the versions, the jobs bar,
//! grouping) answered by the engine.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use session_engine::view::WaveRead;
use session_engine::view_client;

/// Run `id` on the engine over the window's link; its answer as JSON.
fn run(id: &str, args: &Value) -> Result<Value, String> {
    let out = view_client::run(id, args.to_string().as_bytes())?;
    if out.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&out).map_err(|e| format!("{id}: {e}: {out}"))
}

/// `check` true within `secs`, polled — a replica fed by a stream.
fn within(secs: u64, what: &str, check: impl Fn() -> bool) -> Result<(), String> {
    let began = Instant::now();
    while began.elapsed() < Duration::from_secs(secs) {
        if check() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err(format!("{what}: not within {secs} s"))
}

/// A song of one track with one audio item (a committed DI recording).
fn song(dir: &Path) -> Result<PathBuf, String> {
    // A copy beside the song: reading it writes its peaks beside it.
    let committed = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../features/rigs/guitar/default-config/di/riff.wav");
    let wav = dir.join("riff.wav");
    std::fs::copy(&committed, &wav).map_err(|e| format!("{}: {e}", committed.display()))?;
    let text = format!(
        r#"<REAPER_PROJECT 0.1 "7.0/test" 1700000000
  SAMPLERATE 48000 0 0
  TEMPO 120 4 4 0
  <TRACK {{0B5E0000-0000-0000-0000-000000000001}}
    NAME Guitar
    <ITEM
      POSITION 0
      LENGTH 2
      IGUID {{0B5E0000-0000-0000-0000-0000000000A1}}
      NAME riff
      GUID {{0B5E0000-0000-0000-0000-0000000000B1}}
      <SOURCE WAVE
        FILE "{}"
      >
    >
  >
>
"#,
        wav.display()
    );
    let path = dir.join("Linked.RPP");
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path)
}

#[test]
fn a_window_on_the_in_memory_link_gets_the_set_waves_lanes_overlay_and_controls()
-> Result<(), String> {
    // SAFETY: set before the engine starts any thread of its own; this
    // test's process starts the engine once.
    unsafe {
        std::env::set_var("FTS_MCP", "0");
        std::env::set_var("FTS_ENGINE_SILENT", "1");
    }
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = song(dir.path())?;
    session_engine::open::link_window_here().map_err(|e| e.to_string())?;
    assert!(view_client::is_linked());

    // The set: opened by the engine, followed by the window.
    run(
        "SESSION_OPEN",
        &json!({ "path": path.display().to_string() }),
    )?;
    within(60, "the set shown", || {
        view_client::peek_set(|v| v.setlist.as_ref().is_some_and(|s| s.songs.len() == 1))
    })?;
    let song = view_client::peek_set(|v| {
        v.setlist
            .as_ref()
            .and_then(|s| s.songs.first())
            .map(|s| s.project.clone())
    })
    .ok_or("no song")?;
    let item = "{0B5E0000-0000-0000-0000-0000000000A1}".to_owned();

    // Its waveform, as the engine draws it.
    let waves = view_client::waves(&song, vec![item.clone()]).ok_or("no waves answer")?;
    match waves.as_slice() {
        [WaveRead::Whole { item: got, wave }] => {
            assert_eq!(got, &item);
            assert!(wave.points.len() > 10, "{} points", wave.points.len());
            assert!(
                wave.points.iter().any(|(max, _)| *max > 0.01),
                "silent wave"
            );
        }
        other => return Err(format!("waves: {other:?}")),
    }

    // A volume lane the engine wrote, read as the window draws it.
    run(
        "AUTOMATION_WRITE",
        &json!({ "track": "Guitar", "envelope": "Volume",
                 "points": [{ "at": "0s", "value": 0.0 }, { "at": "1s", "value": -6.0 }] }),
    )?;
    let read = view_client::automation(&song).ok_or("no automation answer")?;
    let volume = read
        .lanes
        .iter()
        .find(|l| l.name.to_ascii_lowercase().contains("volume"))
        .ok_or_else(|| {
            format!(
                "no volume lane in {:?}",
                read.lanes.iter().map(|l| &l.name).collect::<Vec<_>>()
            )
        })?;
    assert!(volume.points.len() >= 2, "{} points", volume.points.len());

    // A take volume curve the window's own edit wrote — one batch over
    // the link — shown on the item.
    let curve = vec![
        daw_proto::automation::EnvelopePoint {
            index: 0,
            time: daw_proto::primitives::PositionInSeconds::from_seconds(0.0),
            value: 1.0,
            shape: daw_proto::automation::EnvelopeShape::Linear,
            tension: 0.0,
            selected: false,
        },
        daw_proto::automation::EnvelopePoint {
            index: 1,
            time: daw_proto::primitives::PositionInSeconds::from_seconds(1.0),
            value: 0.25,
            shape: daw_proto::automation::EnvelopeShape::Linear,
            tension: 0.0,
            selected: false,
        },
    ];
    let rt = session_engine::open::engine_runtime().map_err(|e| e.to_string())?;
    let applied = rt
        .block_on(view_client::apply(
            song.clone(),
            vec![session_engine::engine::Edit::SetTakeVolume(
                item.clone(),
                curve,
            )],
        ))
        .ok_or("the edit was not applied")?;
    assert!(applied.items, "a take's volume is an item's");
    run(
        "AUTOMATION_SHOW",
        &json!({ "track": "Guitar", "envelope": "Item volume", "shown": true }),
    )?;
    let read = view_client::automation(&song).ok_or("no automation answer")?;
    let take = read
        .takes
        .iter()
        .find(|(i, _)| *i == item)
        .ok_or("no take curve on the item")?;
    assert_eq!(take.1.points.len(), 2);

    // The hit overlay of the song on screen came over its stream.
    within(30, "the overlay", || {
        session_engine::hit_overlay::current().is_some()
    })?;

    // The palette is the engine's list, what can run now as it says.
    let palette = session_engine::actions::palette();
    assert!(
        palette
            .iter()
            .any(|a| a.id == "SESSION_PLAY_STOP" && a.enabled),
        "{} palette rows",
        palette.len()
    );
    // The versions panel's read is answered by the engine.
    assert!(
        session_engine::actions::versions().is_some(),
        "no versions answer"
    );
    // Grouping is set where the mix is.
    assert_eq!(
        run("SESSION_GROUPING", &json!({ "on": false }))?,
        json!(false)
    );
    run("SESSION_GROUPING", &json!({ "on": true }))?;
    // A job the engine runs reaches the window's bottom bar over its
    // stream, and goes when it ends.
    let job = session_engine::jobs::start("Linked probe", 3);
    within(10, "the job shown", || {
        view_client::jobs()
            .iter()
            .any(|j| j.label == "Linked probe")
    })?;
    drop(job);
    within(10, "the job gone", || {
        !view_client::jobs()
            .iter()
            .any(|j| j.label == "Linked probe")
    })?;

    // What the window selects is what the engine's actions act on: told
    // over the link (nothing here writes the engine's state).
    let shown = session_engine::edit_state::Edit {
        at: 0.5,
        selection: Some(session_engine::edit_state::Span::between(0.5, 1.5)),
    };
    view_client::show_edit(
        shown,
        Some(editing::scope::Scope {
            tracks: vec!["Guitar".to_owned()],
            spans: Vec::new(),
        }),
    );
    within(10, "the window's cursor on the engine", || {
        session_engine::edit_state::of(&song).is_some_and(|e| e.at == 0.5 && e.selection.is_some())
    })?;
    // A guitar has no editing profile: the engine says so, naming what the
    // window selected.
    let scope = match view_client::run("EDIT_SCOPE", b"") {
        Ok(said) | Err(said) => said,
    };
    assert!(scope.contains("Guitar"), "EDIT_SCOPE: {scope}");
    // Where an action puts the time selection comes back over the link:
    // the window takes it up as an ask of its own (one revision more).
    let before = session_engine::edit_state::revision();
    run(
        "TIME_SELECTION_SET",
        &json!({ "from": "1s", "to": "1.75s" }),
    )?;
    within(10, "the engine's ask taken up by the window", || {
        session_engine::edit_state::revision() >= before + 2
    })?;

    // A point of interest the engine writes reaches the window with the
    // song's marks (one step for the engine's, one for the window's).
    let before = session_engine::poi::version();
    run("POI_ADD", &json!({ "at": "1s", "note": "Linked probe" }))?;
    within(10, "the point of interest shown", || {
        session_engine::poi::version() >= before + 2
            && session_engine::poi::shown()
                .is_some_and(|p| p.iter().any(|p| p.note == "Linked probe"))
    })?;
    // An undo the engine does is counted by the window too.
    let before = session_engine::actions::undone();
    run("SESSION_UNDO", &json!({}))?;
    within(10, "the undo counted", || {
        session_engine::actions::undone() >= before + 2
    })?;

    // Closed by the engine: the window's set goes with it.
    run("SESSION_CLOSE", &Value::Null)?;
    within(30, "the set closed", || {
        view_client::peek_set(|v| v.setlist.is_none())
    })?;
    Ok(())
}
