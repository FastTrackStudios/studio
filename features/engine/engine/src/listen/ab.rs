//! `LISTEN_AB`: a point listened to, a change made, the point listened to
//! again, and the change undone unless it is kept — what an agent dialling
//! in a mix does dozens of times a minute, in one call.
//!
//! The change is any list of actions (FX_SET, SIGNAL_RUN, TRACK_SET, …),
//! run inside one undo step. Putting it back is not an undo (undoing
//! through the song's document takes seconds on a big song, and neither a
//! plugin's held values nor its own structural edits — Signal's
//! SIGNAL_RUN — are reliably undo steps): the mix as it was before the
//! change, kept in memory as MIX_SNAPSHOT keeps one, is put back as
//! MIX_RESTORE puts one back — only what differs.

use architect::action::ActionList;
use daw::service::{ProjectContext, Projects};
use daw_standalone::sync::Standalone;
use serde_json::{Value, json};

/// The undo step a change is made in.
pub const STEP: &str = "LISTEN_AB change";

/// Run `calls` on `list` in order, each one's answer (or error) said;
/// stops at the first that fails.
///
/// # Errors
/// The first call that failed, with what the ones before it said.
pub fn run(
    list: &ActionList,
    calls: &[(String, String)],
) -> Result<Vec<Value>, (Vec<Value>, String)> {
    let mut said = Vec::new();
    for (action, args) in calls {
        match list.run_json(action, args.as_bytes()) {
            Ok(bytes) => said.push(json!({
                "action": action,
                "answer": serde_json::from_slice::<Value>(&bytes)
                    .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned())),
            })),
            Err(e) => return Err((said, format!("{action}: {e}"))),
        }
    }
    Ok(said)
}

/// Make `calls` on song `project` as one undo step (the edit history
/// shows the change; putting it back is [`put_back`], not an undo).
///
/// # Errors
/// As [`run`]; the step is closed either way.
pub fn change(
    daw: &Standalone,
    project: &ProjectContext,
    list: &ActionList,
    calls: &[(String, String)],
) -> Result<Vec<Value>, (Vec<Value>, String)> {
    Projects::begin_undo_block(daw, project.clone(), STEP);
    let said = run(list, calls);
    Projects::end_undo_block(daw, project.clone(), STEP, None);
    said
}

/// What putting a change back did.
#[derive(Debug)]
pub struct Back {
    /// What was set again (tracks, sends, FX), as MIX_RESTORE says it.
    pub changed: Vec<String>,
    /// Plugins whose state was loaded back.
    pub states: u32,
}

/// Put the mix back as `before` (taken by `mix_snapshot::current` before
/// the change) has it: every fader, pan, mute, solo, polarity, send, FX
/// switch, held parameter value and plugin state that differs.
///
/// # Errors
/// Something would not take its setting back.
pub fn put_back(
    daw: &Standalone,
    project: &ProjectContext,
    before: &crate::mix_snapshot::Mix,
) -> Result<Back, String> {
    let row = crate::mix_snapshot::put(daw, project, "before LISTEN_AB", before)?;
    Ok(Back {
        changed: row.changed,
        states: row.states_loaded,
    })
}

/// The difference of two LISTEN `figures` (B − A), field by field: every
/// number, and the bands band by band.
#[must_use]
pub fn difference(a: &Value, b: &Value) -> Value {
    let r2 = |x: f64| (x * 100.0).round() / 100.0;
    let mut out = serde_json::Map::new();
    let (Some(a), Some(b)) = (a.as_object(), b.as_object()) else {
        return Value::Null;
    };
    for (k, va) in a {
        let Some(vb) = b.get(k) else { continue };
        let d = match (va, vb) {
            (Value::Number(x), Value::Number(y)) => {
                json!(r2(y.as_f64().unwrap_or(0.0) - x.as_f64().unwrap_or(0.0)))
            }
            (Value::Array(x), Value::Array(y)) => json!(
                x.iter()
                    .zip(y)
                    .map(|(x, y)| r2(y.as_f64().unwrap_or(0.0) - x.as_f64().unwrap_or(0.0)))
                    .collect::<Vec<_>>()
            ),
            _ => continue,
        };
        out.insert(k.clone(), d);
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_difference_is_b_minus_a_number_by_number_and_band_by_band() {
        let a = json!({"lufs": -20.0, "bands": [-30.0, -40.0], "note": "x"});
        let b = json!({"lufs": -21.5, "bands": [-29.0, -40.5]});
        assert_eq!(
            difference(&a, &b),
            json!({"lufs": -1.5, "bands": [1.0, -0.5]})
        );
    }
}
