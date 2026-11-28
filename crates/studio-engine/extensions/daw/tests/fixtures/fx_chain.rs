//! A track's FX chain, edited in every way the window's chain close-up
//! edits it, over MCP (studio#98): an FX added, moved, bypassed, one of
//! its parameters set by index as the generic panel sets it (`=0.25`),
//! each read back; then removed, and every step taken back with
//! SESSION_UNDO — the song saved first, so undo runs through its project
//! file, which keeps the chain.

use serde_json::{Value, json};

use crate::check::Checks;
use crate::mcp::Mcp;

/// What this test puts on the kick's In mic, found by SESSION_TRACKS (the
/// open files a piece's mics under it: `Drums/Kick/Sum/In`).
const FX: &str = "FTS Gate";

/// The path SESSION_TRACKS gives the kick's In mic.
fn kick_in(mcp: &Mcp) -> Result<String, String> {
    let tracks = mcp.run("SESSION_TRACKS", &json!({}))?;
    rows(&tracks)
        .iter()
        .filter_map(|t| t["path"].as_str())
        .find(|path| {
            let mut parts = path.split('/');
            parts.next_back() == Some("In") && parts.any(|p| p == "Kick")
        })
        .map(str::to_owned)
        .ok_or_else(|| format!("SESSION_TRACKS has no Kick/…/In: {tracks}"))
}

/// What the generic panel writes: a normalized value, by index.
const VALUE: f64 = 0.25;

/// An answer's rows: a list, or the list it carries.
fn rows(v: &Value) -> Vec<Value> {
    v.as_array()
        .or_else(|| v["rows"].as_array())
        .or_else(|| v["result"].as_array())
        .cloned()
        .unwrap_or_default()
}

/// The track's chain, in order (`FX_LIST`).
fn chain(mcp: &Mcp, track: &str) -> Result<Vec<Value>, String> {
    Ok(rows(&mcp.run("FX_LIST", &json!({ "track": track }))?))
}

/// The chain's names, in order.
fn names(chain: &[Value]) -> Vec<String> {
    chain
        .iter()
        .map(|fx| fx["name"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// Where the FX `guid` is in `chain` (from 0), and whether it is on.
fn place_of(chain: &[Value], guid: &str) -> Option<(usize, bool)> {
    chain
        .iter()
        .position(|fx| fx["guid"] == json!(guid))
        .map(|at| (at, chain[at]["on"] == json!(true)))
}

/// The first parameter of `fx` a slider sets (no choices), and its value.
fn a_continuous_param(mcp: &Mcp, track: &str, fx: &str) -> Result<(u64, f64), String> {
    let params = rows(&mcp.run("FX_PARAMS", &json!({ "track": track, "fx": fx }))?);
    params
        .iter()
        .find(|p| p["choices"].as_array().is_none_or(Vec::is_empty))
        .and_then(|p| Some((p["index"].as_u64()?, p["normalized"].as_f64()?)))
        .ok_or_else(|| format!("{fx} has no continuous parameter: {params:?}"))
}

/// Parameter `index` of `fx`, normalized.
fn param(mcp: &Mcp, track: &str, fx: &str, index: u64) -> Result<Option<f64>, String> {
    let params = rows(&mcp.run("FX_PARAMS", &json!({ "track": track, "fx": fx }))?);
    Ok(params
        .iter()
        .find(|p| p["index"].as_u64() == Some(index))
        .and_then(|p| p["normalized"].as_f64()))
}

/// Add, move, bypass, set, remove, and undo it all.
pub fn edited_and_undone(mcp: &Mcp, checks: &mut Checks) -> Result<(), String> {
    // Undo through the song's file, which keeps every FX in the chain.
    mcp.run("SESSION_SAVE", &json!({}))?;
    let track = kick_in(mcp)?;
    let before = names(&chain(mcp, &track)?);

    mcp.run("FX_ADD", &json!({ "track": track, "fx": FX }))?;
    let added = chain(mcp, &track)?;
    let guid = added
        .iter()
        .rev()
        .find(|fx| fx["name"] == json!(FX))
        .and_then(|fx| fx["guid"].as_str())
        .ok_or_else(|| format!("{FX} not in the chain after FX_ADD: {added:?}"))?
        .to_owned();
    checks.add(
        "added at the end",
        place_of(&added, &guid) == Some((before.len(), true)),
        format!("{:?}", names(&added)),
    );

    mcp.run("FX_MOVE", &json!({ "track": track, "fx": guid, "to": 1 }))?;
    let after_move = chain(mcp, &track)?;
    checks.add(
        "moved first",
        place_of(&after_move, &guid).map(|(at, _)| at) == Some(0),
        format!("{:?}", names(&after_move)),
    );

    mcp.run(
        "FX_ENABLE",
        &json!({ "track": track, "fx": guid, "on": false }),
    )?;
    let bypassed = chain(mcp, &track)?;
    checks.add(
        "bypassed",
        place_of(&bypassed, &guid) == Some((0, false)),
        format!("{:?}", place_of(&bypassed, &guid)),
    );

    let (index, was) = a_continuous_param(mcp, &track, &guid)?;
    let key = format!("#{index}");
    mcp.run(
        "FX_SET",
        &json!({ "track": track, "fx": guid, "set": { key: format!("={VALUE}") } }),
    )?;
    let now = param(mcp, &track, &guid, index)?;
    checks.add(
        "a parameter set as the generic panel sets it",
        now.is_some_and(|v| (v - VALUE).abs() < 0.02),
        format!("#{index}: {now:?} (was {was})"),
    );

    let removed = mcp.run("FX_REMOVE", &json!({ "track": track, "fx": guid }))?;
    let gone = chain(mcp, &track)?;
    checks.add(
        "removed",
        place_of(&gone, &guid).is_none() && names(&gone) == before,
        format!("{removed}; {:?}", names(&gone)),
    );

    // Undo the remove: back first in the chain, bypassed, as it was set.
    let undone = mcp.run("SESSION_UNDO", &json!({}))?;
    let back = chain(mcp, &track)?;
    let first = back.first();
    checks.add(
        "undo puts it back where it was, bypassed",
        first.is_some_and(|fx| fx["name"] == json!(FX) && fx["on"] == json!(false)),
        format!("{undone}; {:?}", names(&back)),
    );
    if let Some(fx) = first.and_then(|fx| fx["guid"].as_str()) {
        let kept = param(mcp, &track, fx, index)?;
        checks.add(
            "and as it was set",
            kept.is_some_and(|v| (v - VALUE).abs() < 0.02),
            format!("#{index}: {kept:?}"),
        );
    }

    // The set, the bypass, the move and the add, taken back in turn.
    let mut said = Vec::new();
    for _ in 0..4 {
        said.push(mcp.run("SESSION_UNDO", &json!({}))?.to_string());
    }
    let last = names(&chain(mcp, &track)?);
    checks.add(
        "every step undone",
        last == before,
        format!("{said:?}; {last:?}, {before:?} before"),
    );
    Ok(())
}
