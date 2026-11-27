//! The tempo and the meter set from the window, over MCP (studio#98,
//! studio#97): a tempo change and a 3/4 bar made with the ruler's gesture
//! action (`ARRANGE_TEMPO`, what the TEMPO lane commits), the bars and
//! beats read back as the window's ruler reads them (`SESSION_TEMPO`),
//! undone and redone as one step, and a tempo set elsewhere (`TEMPO_SET`)
//! read back the same way with no reopening.

use serde_json::{Value, json};

use crate::check::Checks;
use crate::mcp::Mcp;

/// The undo step the ruler's tempo gestures make (`tempo_edit::LABEL`).
const LABEL: &str = "Edit the tempo map";

/// The tempo map as the window reads it, and the bars of `at`.
fn read(mcp: &Mcp, at: &[f64]) -> Result<Value, String> {
    mcp.run("SESSION_TEMPO", &json!({ "at": at }))
}

/// The changes `read` says.
fn changes(said: &Value) -> Vec<Value> {
    said["changes"].as_array().cloned().unwrap_or_default()
}

/// The change at `at` seconds, if there is one.
fn change_at(said: &Value, at: f64) -> Option<Value> {
    changes(said)
        .into_iter()
        .find(|c| c["at"].as_f64().is_some_and(|t| (t - at).abs() < 0.002))
}

/// A bar as `read` says it (`5.2` or `5.2.250`): its bar and beat.
fn bar_beat(text: &str) -> Option<(i64, i64)> {
    let mut parts = text.split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// The bar and beat of the `n`th time asked of `read`.
fn bar_of(said: &Value, n: usize) -> Option<(i64, i64)> {
    said["bars"].get(n)?["bar"].as_str().and_then(bar_beat)
}

fn near(v: &Value, want: f64) -> bool {
    v.as_f64().is_some_and(|got| (got - want).abs() <= 0.01)
}

/// Set a tempo and a 3/4 bar from the window's action, read the bars back,
/// undo and redo it, then set a tempo over MCP and read it back.
pub fn from_the_window(mcp: &Mcp, checks: &mut Checks) -> Result<(), String> {
    let before = read(mcp, &[])?;
    let first = changes(&before)
        .into_iter()
        .next()
        .ok_or("the song has no tempo")?;
    let bpm = first["bpm"].as_f64().ok_or("no tempo at the start")?;
    let count = changes(&before).len();
    // Two bars of 4/4 in: where the window makes its change.
    let at = 8.0 * 60.0 / bpm;
    checks.add(
        "a tempo to start from",
        bpm > 0.0 && first["meter"] == json!("4/4"),
        format!("{bpm} bpm, {}, {count} change(s)", first["meter"]),
    );

    // What the TEMPO lane commits for a change dragged to 90 bpm, then its
    // menu's 3/4: one gesture each, as the window sends them.
    for edits in [
        json!([{ "tempo_set": { "at": at, "bpm": 90.0 } }]),
        json!([{ "meter_set": { "at": at, "numerator": 3, "denominator": 4 } }]),
    ] {
        mcp.run("ARRANGE_TEMPO", &json!({ "label": LABEL, "edits": edits }))?;
    }
    let (beat, bar) = (60.0 / 90.0, 3.0 * 60.0 / 90.0);
    // Just past each beat, so a rounding cannot read a downbeat as the end
    // of the beat before it.
    let past = |t: f64| t + 0.01;
    let said = read(
        mcp,
        &[
            past(at),
            past(at + beat),
            past(at + bar),
            past(at + 2.0 * bar),
        ],
    )?;
    let made = change_at(&said, at);
    checks.add(
        "a tempo and a meter from the window",
        made.as_ref()
            .is_some_and(|c| near(&c["bpm"], 90.0) && c["meter"] == json!("3/4")),
        format!("{made:?}"),
    );
    let bars: Vec<Option<(i64, i64)>> = (0..4).map(|n| bar_of(&said, n)).collect();
    let counted = match bars.as_slice() {
        [Some((b0, 1)), Some((b1, 2)), Some((b2, 1)), Some((b3, 1))] => {
            b1 == b0 && *b2 == b0 + 1 && *b3 == b0 + 2
        }
        _ => false,
    };
    checks.add(
        "bars and beats in 3/4 at 90",
        counted,
        format!("{:?}", said["bars"]),
    );

    // Undo takes the meter back, then the tempo; redo brings both back.
    mcp.run("SESSION_UNDO", &json!({}))?;
    let once = read(mcp, &[])?;
    checks.add(
        "undo takes the meter back",
        change_at(&once, at).is_some_and(|c| c["meter"] == json!("4/4") && near(&c["bpm"], 90.0)),
        format!("{:?}", change_at(&once, at)),
    );
    mcp.run("SESSION_UNDO", &json!({}))?;
    let twice = read(mcp, &[])?;
    checks.add(
        "and then the tempo",
        change_at(&twice, at).is_none() && changes(&twice).len() == count,
        format!(
            "{} change(s): {:?}",
            changes(&twice).len(),
            twice["changes"]
        ),
    );
    mcp.run("SESSION_REDO", &json!({}))?;
    mcp.run("SESSION_REDO", &json!({}))?;
    let again = read(mcp, &[])?;
    checks.add(
        "redo brings both back",
        change_at(&again, at).is_some_and(|c| near(&c["bpm"], 90.0) && c["meter"] == json!("3/4")),
        format!("{:?}", change_at(&again, at)),
    );

    // A tempo set elsewhere — over MCP, as a model or a member would —
    // four bars of 3/4 on: read back as the window reads it, in 3/4 still.
    let later = at + 4.0 * bar;
    mcp.run(
        "TEMPO_SET",
        &json!({ "bpm": 140.0, "at": format!("{later}s"), "follow": false }),
    )?;
    let after = read(mcp, &[past(later)])?;
    let set = change_at(&after, later);
    checks.add(
        "a tempo set over MCP reads back",
        set.as_ref()
            .is_some_and(|c| near(&c["bpm"], 140.0) && c["meter"] == json!("3/4"))
            && bar_of(&after, 0).is_some_and(|(_, beat)| beat == 1),
        format!("{set:?} on {:?}", after["bars"]),
    );
    Ok(())
}
