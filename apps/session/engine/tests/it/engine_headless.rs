//! The engine with no window, driven through its Action List alone — what
//! `studio engine` and the CLI do. Nothing here opens an audio device or a
//! window: a new song is made in the engine, its edit state set and read
//! back, and the Live roll-on switched. (A window attaching to it is
//! session-ui's test, `window_attach`.)

use serde_json::{Value, json};

/// Run `id` with `args` on the engine's list; its answer as JSON.
fn run(id: &str, args: &Value) -> Result<Value, String> {
    let list = session_engine::actions::list();
    let out = list
        .run_json(id, args.to_string().as_bytes())
        .map_err(|e| format!("{id}: {e}"))?;
    if out.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&out).map_err(|e| format!("{id}: {e}"))
}

/// Wait (at most 60 s) for the engine's set to be what `done` says.
fn opened(done: impl Fn(&[Value]) -> bool) -> Result<(), String> {
    let began = std::time::Instant::now();
    while began.elapsed() < std::time::Duration::from_secs(60) {
        let idle = run("SESSION_OPENING", &Value::Null)?["state"] == json!("idle");
        let songs = run("SESSION_SONGS", &Value::Null)?;
        if idle && done(songs.as_array().map_or(&[], Vec::as_slice)) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Err("the engine's set did not open".to_owned())
}

fn ids() -> Vec<(String, bool)> {
    session_engine::actions::list()
        .infos()
        .into_iter()
        .map(|i| (i.id, i.enabled))
        .collect()
}

fn listed(id: &str) -> Option<bool> {
    ids().into_iter().find(|(i, _)| i == id).map(|(_, on)| on)
}

#[test]
fn a_headless_engine_is_driven_through_its_action_list() -> Result<(), String> {
    // No MCP port for a test: the list is driven in process.
    // SAFETY: set before the engine starts any thread of its own; this
    // binary's only test that starts the engine.
    unsafe {
        std::env::set_var("FTS_MCP", "0");
        std::env::set_var("FTS_ENGINE_SILENT", "1");
    }
    let engine = session_engine::studio_engine::headless::start().map_err(|e| e.to_string())?;
    drop(engine);

    // The engine's own actions are there with no window; the window's are
    // not, until a window attaches.
    assert_eq!(listed("SESSION_SONGS"), Some(true));
    assert_eq!(listed("SESSION_PLAY_STOP"), Some(true));
    assert_eq!(
        listed("SESSION_ZOOM"),
        None,
        "a window's action, with no window"
    );
    assert_eq!(
        run("SESSION_OPENING", &Value::Null)?["state"],
        json!("idle")
    );

    // A song made in the engine (no file, no audio) — as its set, opened on
    // a thread of its own — and its edit state set and read back.
    run("SESSION_NEW", &json!({}))?;
    opened(|songs| songs.len() == 1)?;
    run("TIME_SELECTION_SET", &json!({"from": "1s", "to": "2s"}))?;
    let edit = run("SESSION_EDIT_STATE", &Value::Null)?;
    let seconds = |key: &str| edit[key].as_f64();
    assert_eq!(
        (
            seconds("cursor"),
            seconds("selection_start"),
            seconds("selection_end")
        ),
        (Some(1.0), Some(1.0), Some(2.0)),
        "{edit}"
    );

    // Live mode's roll-on is the engine's.
    assert_eq!(
        run("SESSION_LIVE", &json!({"on": true}))?,
        json!("live: on")
    );
    assert_eq!(run("SESSION_LIVE", &json!({}))?, json!("live: on"));
    assert_eq!(
        run("SESSION_LIVE", &json!({"on": false}))?,
        json!("live: off")
    );

    Ok(())
}
