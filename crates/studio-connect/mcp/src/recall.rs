//! A Signal instance's block meters, kept across LISTEN's replays.
//!
//! LISTEN answers a point it heard before, with nothing upstream changed,
//! by playing its recording back: nothing renders, so its Signal
//! instances' `dynamics[].blocks` (their blocks' own meters, read off the
//! render) come back null with `blocks_why`. The bridge keeps the meters
//! of every point it saw rendered and puts them back into a replay of the
//! same point — same track, cut, span, solos and `exact` — when the
//! replay's figures are the rendered ones to the last bit (a replay plays
//! the same samples back, so they are; anything else means the point was
//! rendered since by someone else, and its meters stay null).
//!
//! The engine should keep the meters with its recordings itself
//! (`apps/studio/docs/mcp.md`, "Block meters on a replay"); this is the
//! bridge's stopgap and costs nothing once it does.

use std::sync::{Mutex, PoisonError};

use serde_json::{Value, json};

/// How many (point, FX) meters are kept, the oldest let go first.
const KEEP: usize = 512;

/// What a replayed answer says in place of the meters, once filled.
pub const FILLED: &str = "remembered from the render that heard this point unchanged";

struct Seen {
    key: String,
    figures: String,
    blocks: Value,
}

static SEEN: Mutex<Vec<Seen>> = Mutex::new(Vec::new());

/// The figures as a replay must give them again: every number, exactly.
fn signature(point: &Value) -> String {
    let mut f = point.get("figures").cloned().unwrap_or(Value::Null);
    if let Some(o) = f.as_object_mut() {
        o.remove("short_term");
    }
    f.to_string()
}

fn key(point: &Value, span: &Value, args: &Value, fx: &Value) -> String {
    json!([
        point.get("track"),
        point.get("at"),
        span.get("from"),
        span.get("to"),
        args.get("solo"),
        args.get("exact").and_then(Value::as_bool).unwrap_or(false),
        fx,
    ])
    .to_string()
}

fn remember(key: String, figures: String, blocks: Value) {
    let mut seen = SEEN.lock().unwrap_or_else(PoisonError::into_inner);
    seen.retain(|s| s.key != key);
    if seen.len() >= KEEP {
        seen.remove(0);
    }
    seen.push(Seen {
        key,
        figures,
        blocks,
    });
}

fn recalled(key: &str, figures: &str) -> Option<Value> {
    let seen = SEEN.lock().unwrap_or_else(PoisonError::into_inner);
    seen.iter()
        .find(|s| s.key == key && s.figures == figures)
        .map(|s| s.blocks.clone())
}

/// Remember, or put back, the block meters of one point's answer.
fn point(point: &mut Value, span: &Value, args: &Value) {
    let figures = signature(point);
    let snapshot = point.clone();
    let Some(dynamics) = point.get_mut("dynamics").and_then(Value::as_array_mut) else {
        return;
    };
    for d in dynamics {
        let k = key(&snapshot, span, args, d.get("fx").unwrap_or(&Value::Null));
        match d.get("blocks") {
            Some(b) if b.is_array() => remember(k, figures.clone(), b.clone()),
            Some(Value::Null) => {
                if let (Some(blocks), Some(o)) = (recalled(&k, &figures), d.as_object_mut()) {
                    o.insert("blocks".into(), blocks);
                    o.remove("blocks_why");
                    o.insert("blocks_from".into(), json!(FILLED));
                }
            }
            _ => {}
        }
    }
}

/// A `LISTEN` or `LISTEN_BLOCK` answer (`id`) with its replayed block meters
/// put back where the bridge saw them rendered, and its rendered ones
/// remembered. `args` are what the call was given.
pub fn block_meters(id: &str, answer: &mut Value, args: &Value) {
    let span = json!({"from": answer.get("from"), "to": answer.get("to")});
    match id {
        "LISTEN" => {
            if let Some(ps) = answer.get_mut("points").and_then(Value::as_array_mut) {
                for p in ps {
                    point(p, &span, args);
                }
            } else {
                point(answer, &span, args);
            }
        }
        "LISTEN_BLOCK" => {
            for side in ["pre", "post"] {
                if let Some(p) = answer.get_mut(side) {
                    point(p, &span, args);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn heard(lufs: f64, blocks: &Value) -> Value {
        json!({"track": "Recall/In", "at": "post-fader", "from": 1.0, "to": 2.0,
               "figures": {"lufs": lufs, "short_term": [lufs]},
               "dynamics": [{"fx": "#1 Signal", "max_gr_db": -1.0, "blocks": blocks,
                             "blocks_why": if blocks.is_null() { json!("replayed") } else { Value::Null }}]})
    }

    #[test]
    fn a_replay_gets_the_meters_its_render_had() {
        let meters = json!([{"block": "tone.comp", "meters": []}]);
        let args = json!({"track": "Recall/In"});
        let mut first = heard(-20.0, &meters);
        block_meters("LISTEN", &mut first, &args);
        let mut again = heard(-20.0, &Value::Null);
        block_meters("LISTEN", &mut again, &args);
        assert_eq!(again["dynamics"][0]["blocks"], meters);
        assert_eq!(again["dynamics"][0]["blocks_from"], json!(FILLED));
        assert!(again["dynamics"][0].get("blocks_why").is_none());
    }

    #[test]
    fn other_figures_or_solos_get_nothing() {
        let meters = json!([{"block": "tone.gate", "meters": []}]);
        let args = json!({"track": "Recall/In", "solo": ["A"]});
        let mut first = heard(-30.0, &meters);
        block_meters("LISTEN", &mut first, &args);
        let mut changed = heard(-31.0, &Value::Null);
        block_meters("LISTEN", &mut changed, &args);
        assert!(changed["dynamics"][0]["blocks"].is_null());
        let mut unsoloed = heard(-30.0, &Value::Null);
        block_meters("LISTEN", &mut unsoloed, &json!({"track": "Recall/In"}));
        assert!(unsoloed["dynamics"][0]["blocks"].is_null());
    }
}
