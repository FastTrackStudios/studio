//! A send, made and changed in every parameter the window's routing
//! close-up has, over MCP (studio#98): added, its level, pan, mute and tap
//! set, read from both ends (a receive is the send feeding it), and taken
//! off — each read back over the same door.

use serde_json::{Value, json};

use crate::check::Checks;
use crate::mcp::Mcp;

/// The send this test makes: the kick's In mic to the overhead, by name;
/// their paths are what SESSION_TRACKS says once the kit is open (the open
/// files a piece's mics under it: `Drums/Kick/Sum/In`).
const KICK_IN: (&str, &str) = ("Kick", "In");
const OVERHEAD: (&str, &str) = ("Drums", "OH");

/// The path SESSION_TRACKS gives the track `name` inside a folder `under`.
fn path_of(mcp: &Mcp, (under, name): (&str, &str)) -> Result<String, String> {
    let tracks = mcp.run("SESSION_TRACKS", &json!({}))?;
    let rows = tracks
        .as_array()
        .or_else(|| tracks["rows"].as_array())
        .cloned()
        .unwrap_or_default();
    rows.iter()
        .filter_map(|t| t["path"].as_str())
        .find(|path| {
            let mut parts = path.split('/');
            parts.next_back() == Some(name) && parts.any(|p| p == under)
        })
        .map(str::to_owned)
        .ok_or_else(|| format!("SESSION_TRACKS has no {under}/…/{name}: {tracks}"))
}

/// `track`'s sends and the sends into it (`SENDS_LIST`).
fn sends(mcp: &Mcp, track: &str) -> Result<Vec<Value>, String> {
    let said = mcp.run("SENDS_LIST", &json!({ "track": track }))?;
    Ok(said["sends"].as_array().cloned().unwrap_or_default())
}

/// The send from `from` to `to` as `track`'s list has it.
fn the_send(mcp: &Mcp, (from, to): (&str, &str), track: &str) -> Result<Option<Value>, String> {
    Ok(sends(mcp, track)?
        .into_iter()
        .find(|s| s["from"] == json!(from) && s["to"] == json!(to)))
}

fn near(v: &Value, want: f64) -> bool {
    v.as_f64().is_some_and(|got| (got - want).abs() <= 0.05)
}

/// What a send must read: its level, pan, mute and tap.
fn reads(send: Option<&Value>, db: f64, pan: f64, mute: bool, tap: &str) -> (bool, String) {
    let Some(s) = send else {
        return (false, "no such send".to_owned());
    };
    let ok = near(&s["level_db"], db)
        && near(&s["pan"], pan)
        && s["mute"] == json!(mute)
        && s["tap"] == json!(tap);
    (
        ok,
        format!(
            "{} dB, pan {}, mute {}, {}",
            s["level_db"], s["pan"], s["mute"], s["tap"]
        ),
    )
}

/// Make, change, read from both ends and remove one send.
pub fn every_parameter(mcp: &Mcp, checks: &mut Checks) -> Result<(), String> {
    let (from_path, to_path) = (path_of(mcp, KICK_IN)?, path_of(mcp, OVERHEAD)?);
    let (from, to) = (from_path.as_str(), to_path.as_str());
    let ends = (from, to);
    let added = mcp.run(
        "SEND_ADD",
        &json!({ "from": from, "to": to, "level_db": -12.0 }),
    )?;
    checks.add(
        "send added",
        added["status"] == json!("added"),
        format!("{}", added["status"]),
    );
    let (ok, said) = reads(
        the_send(mcp, ends, from)?.as_ref(),
        -12.0,
        0.0,
        false,
        "post-fader",
    );
    checks.add("as added", ok, said);

    mcp.run(
        "SEND_SET",
        &json!({
            "from": from, "to": to,
            "level_db": -6.0, "pan": -0.5, "mute": true, "tap": "pre-fx",
        }),
    )?;
    let (ok, said) = reads(
        the_send(mcp, ends, from)?.as_ref(),
        -6.0,
        -0.5,
        true,
        "pre-fx",
    );
    checks.add("every parameter set", ok, said);
    // The same send, listed at the end it feeds.
    let (ok, said) = reads(
        the_send(mcp, ends, to)?.as_ref(),
        -6.0,
        -0.5,
        true,
        "pre-fx",
    );
    checks.add("read from its receive", ok, said);

    mcp.run(
        "SEND_SET",
        &json!({ "from": from, "to": to, "mute": false, "tap": "pre-fader" }),
    )?;
    let (ok, said) = reads(
        the_send(mcp, ends, from)?.as_ref(),
        -6.0,
        -0.5,
        false,
        "pre-fader",
    );
    checks.add("only what is given changes", ok, said);

    let removed = mcp.run("SEND_REMOVE", &json!({ "from": from, "to": to }))?;
    let gone = the_send(mcp, ends, from)?.is_none() && the_send(mcp, ends, to)?.is_none();
    checks.add(
        "send removed",
        removed["removed"] == json!(1) && gone,
        format!("removed {}, still listed: {}", removed["removed"], !gone),
    );
    Ok(())
}
