//! A send, made and changed in every parameter the window's routing
//! close-up has, over MCP (studio#98): added, its level, pan, mute and tap
//! set, read from both ends (a receive is the send feeding it), and taken
//! off — each read back over the same door.

use serde_json::{Value, json};

use crate::check::Checks;
use crate::mcp::Mcp;

/// The send this test makes: a close mic to the overhead.
const FROM: &str = "Kick/In";
const TO: &str = "OH";

/// `track`'s sends and the sends into it (`SENDS_LIST`).
fn sends(mcp: &Mcp, track: &str) -> Result<Vec<Value>, String> {
    let said = mcp.run("SENDS_LIST", &json!({ "track": track }))?;
    Ok(said["sends"].as_array().cloned().unwrap_or_default())
}

/// The send from [`FROM`] to [`TO`] as `track`'s list has it.
fn the_send(mcp: &Mcp, track: &str) -> Result<Option<Value>, String> {
    Ok(sends(mcp, track)?.into_iter().find(|s| {
        s["from"].as_str().is_some_and(|f| f.ends_with(FROM))
            && s["to"].as_str().is_some_and(|t| t.ends_with(TO))
    }))
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
    let added = mcp.run(
        "SEND_ADD",
        &json!({ "from": FROM, "to": TO, "level_db": -12.0 }),
    )?;
    checks.add(
        "send added",
        added["status"] == json!("added"),
        format!("{}", added["status"]),
    );
    let (ok, said) = reads(
        the_send(mcp, FROM)?.as_ref(),
        -12.0,
        0.0,
        false,
        "post-fader",
    );
    checks.add("as added", ok, said);

    mcp.run(
        "SEND_SET",
        &json!({
            "from": FROM, "to": TO,
            "level_db": -6.0, "pan": -0.5, "mute": true, "tap": "pre-fx",
        }),
    )?;
    let (ok, said) = reads(the_send(mcp, FROM)?.as_ref(), -6.0, -0.5, true, "pre-fx");
    checks.add("every parameter set", ok, said);
    // The same send, listed at the end it feeds.
    let (ok, said) = reads(the_send(mcp, TO)?.as_ref(), -6.0, -0.5, true, "pre-fx");
    checks.add("read from its receive", ok, said);

    mcp.run(
        "SEND_SET",
        &json!({ "from": FROM, "to": TO, "mute": false, "tap": "pre-fader" }),
    )?;
    let (ok, said) = reads(
        the_send(mcp, FROM)?.as_ref(),
        -6.0,
        -0.5,
        false,
        "pre-fader",
    );
    checks.add("only what is given changes", ok, said);

    let removed = mcp.run("SEND_REMOVE", &json!({ "from": FROM, "to": TO }))?;
    let gone = the_send(mcp, FROM)?.is_none() && the_send(mcp, TO)?.is_none();
    checks.add(
        "send removed",
        removed["removed"] == json!(1) && gone,
        format!("removed {}, still listed: {}", removed["removed"], !gone),
    );
    Ok(())
}
