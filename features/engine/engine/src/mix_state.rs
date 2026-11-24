//! Everything about a track and the tracks inside it, in one read
//! (`MIX_STATE`): its fader, pan, mute, solo and polarity, how it reaches
//! its parent, its sends and receives, its delay compensation, every FX
//! with every parameter's value and text (a Signal instance's chain as
//! `signal.chain/1` when the engine can ask for it), its automation lanes
//! and their points over a span, and its meter now.
//!
//! What an agent dialling in a mix reads before and after each change:
//! one call in place of FX_LIST, FX_PARAMS per FX, MIX_SENDS, the
//! automation reads and the meters, each of those a walk of its own.

use std::collections::HashMap;

use architect::action::{ActionCaller, ActionList, ActionMeta, Effect, RawJson};
use daw::service::automation::{EnvelopeLocation, EnvelopeRef, EnvelopeType, TimeRangeParams};
use daw::service::{
    Automation, Effects, FxChainContext, FxRef, FxTarget, PositionInSeconds, ProjectContext,
    Routing, TrackRef, Tracks,
};
use daw_standalone::sync::Standalone;
use facet::Facet;
use serde_json::{Value, json};

use crate::actions::{meta, resolve, song};
use crate::mix_sends::{db, paths, tap_word};

static STATE: ActionMeta = meta(
    "MIX_STATE",
    "mix_state",
    "Everything about a track",
    "One read of `track` (GUID, path or name) and every track inside it, `depth` levels down (all of them unless given; 0: the track alone), in track order. Each track: `path`, `level` (0 the track asked for), `kind` (track/folder), `volume_db`, `pan` (-1 left … 1 right), `mute`, `solo`, `phase` (polarity inverted), `to_parent` (whether it feeds its folder), `sends` and `receives` (each the other track's path, `db`, `pan`, `mute`, `tap`: pre-fx, pre-fader or post-fader), `chain_latency` (its FX's delay, samples) and `latency` (how late its output is once delay compensation has lined its inputs up, samples), `meter` (peak and held peak now, dBFS, L and R), `fx` in chain order (each `place` `#1`…, `name`, `on`, `latency`, `parameters` (how many it has) and `params`: one `\"#index name = text (normalized)\"` string a parameter, listed for the FX that are on — `params: true` lists every FX's, `params: false` none, `filter` keeps the names containing it; a Signal instance carries `signal`, its `signal.chain/1`, when SIGNAL_CHAIN is there, its params then listed only with `params: true`), and `automation`: each lane (`lane` — Volume, Pan, Mute, `fx#2 Threshold` — its `mode`, its `points` as `[seconds, value, shape]` between `from` and `to` — bars `33`, `33:2`, times `12.5s`, sections — or every point when they are not given). Read-only; says the time taken (`ms`).",
    "Mix",
    "",
);

/// What to read.
#[derive(Facet)]
struct StateArg {
    track: String,
    depth: Option<u32>,
    from: Option<String>,
    to: Option<String>,
    params: Option<bool>,
    filter: Option<String>,
}

/// Whose FX parameters are listed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Params {
    /// Every FX's.
    All,
    /// None.
    None,
    /// The FX that are on, but a Signal instance whose chain says them.
    On,
}

/// The span automation is read over; `None` for all of it.
type Window = Option<(f64, f64)>;

/// One track's place in the subtree read.
struct Member<'a> {
    track: &'a daw_proto::Track,
    level: u32,
}

/// The tracks inside `root` (itself first, then in track order), at most
/// `depth` levels down.
fn members<'a>(tracks: &'a [daw_proto::Track], root: &str, depth: Option<u32>) -> Vec<Member<'a>> {
    let parent: HashMap<&str, Option<&str>> = tracks
        .iter()
        .map(|t| (t.guid.as_str(), t.parent_guid.as_deref()))
        .collect();
    let level_in_root = |guid: &str| {
        let mut at = Some(guid);
        let mut level = 0u32;
        while let Some(g) = at {
            if g == root {
                return Some(level);
            }
            at = parent.get(g).copied().flatten();
            level += 1;
        }
        None
    };
    tracks
        .iter()
        .filter_map(|t| {
            let level = level_in_root(&t.guid)?;
            depth
                .is_none_or(|d| level <= d)
                .then_some(Member { track: t, level })
        })
        .collect()
}

/// Two decimals.
fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// A route as MIX_STATE says it, `other` the track at its other end.
fn route_json(r: &daw_proto::TrackRoute, other: &str) -> Value {
    json!({
        "track": other,
        "db": r2(db(r.volume)),
        "pan": r2(r.pan),
        "mute": r.muted,
        "tap": tap_word(r.send_mode),
    })
}

/// The latency of each FX on chain `fx` (by guid; 0 for one off or not
/// loaded), samples.
fn fx_latencies(daw: &Standalone, fx: &[daw_proto::fx::Fx]) -> Vec<u32> {
    fx.iter()
        .map(|f| {
            if f.enabled {
                daw.fx_latency_now(&f.guid).unwrap_or(0)
            } else {
                0
            }
        })
        .collect()
}

/// How late each track's output is once delay compensation lines up what
/// feeds it: its latest input (a child or a send) plus its own chain.
/// `inputs` names each track's feeders by guid; `chain` is each track's
/// own delay. Tracks fed by a loop read their own chain only.
fn output_latency(
    order: &[&str],
    inputs: &HashMap<&str, Vec<&str>>,
    chain: &HashMap<&str, u32>,
) -> HashMap<String, u32> {
    fn visit<'a>(
        g: &'a str,
        inputs: &HashMap<&str, Vec<&'a str>>,
        chain: &HashMap<&str, u32>,
        out: &mut HashMap<String, u32>,
        seen: &mut std::collections::HashSet<&'a str>,
    ) -> u32 {
        if let Some(l) = out.get(g) {
            return *l;
        }
        if !seen.insert(g) {
            return chain.get(g).copied().unwrap_or(0);
        }
        let fed = inputs
            .get(g)
            .into_iter()
            .flatten()
            .map(|i| visit(i, inputs, chain, out, seen))
            .max()
            .unwrap_or(0);
        let l = fed.saturating_add(chain.get(g).copied().unwrap_or(0));
        out.insert(g.to_owned(), l);
        l
    }
    let mut out = HashMap::new();
    let mut seen = std::collections::HashSet::new();
    for g in order {
        visit(g, inputs, chain, &mut out, &mut seen);
    }
    out
}

/// What feeds each track: its children that send to it, and every send
/// into it.
fn feeders<'a>(
    tracks: &'a [daw_proto::Track],
    sends: &'a HashMap<String, Vec<daw_proto::TrackRoute>>,
) -> HashMap<&'a str, Vec<&'a str>> {
    let mut inputs: HashMap<&str, Vec<&str>> = HashMap::new();
    for t in tracks {
        if let (Some(p), true) = (t.parent_guid.as_deref(), t.parent_send) {
            inputs.entry(p).or_default().push(&t.guid);
        }
    }
    for (from, routes) in sends {
        for to in routes.iter().filter_map(|r| r.dest_track_guid.as_deref()) {
            inputs.entry(to).or_default().push(from.as_str());
        }
    }
    inputs
}

/// One FX parameter, as one string.
fn param_line(p: &daw_proto::fx::FxParameter) -> String {
    format!("#{} {} = {} ({:.3})", p.index, p.name, p.formatted, p.value)
}

/// `fx`'s `signal.chain/1`, when the engine has SIGNAL_CHAIN and `fx` is
/// a Signal instance.
fn signal_chain(list: &ActionList, fx: &daw_proto::fx::Fx, track_path: &str) -> Option<Value> {
    if !crate::signal_kit::is_signal_bundle(&fx.name) || list.info("SIGNAL_CHAIN").is_none() {
        return None;
    }
    let args = json!({ "track": track_path }).to_string();
    let bytes = list.run_json("SIGNAL_CHAIN", args.as_bytes()).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// A track's FX, as MIX_STATE says them.
fn fx_json(
    daw: &Standalone,
    project: &ProjectContext,
    (track, path): (&str, &str),
    (params, filter): (Params, Option<&str>),
) -> (Vec<Value>, u32) {
    let context = FxChainContext::Track(track.to_owned());
    let chain = Effects::list(daw, project.clone(), context.clone());
    let latencies = fx_latencies(daw, &chain);
    let list = crate::actions::list();
    let rows = chain
        .iter()
        .zip(&latencies)
        .enumerate()
        .map(|(i, (fx, latency))| {
            let signal = signal_chain(list, fx, path);
            let listed = match params {
                Params::All => true,
                Params::None => false,
                Params::On => fx.enabled && signal.is_none(),
            };
            let mut row = json!({
                "place": format!("#{}", i + 1),
                "name": crate::clap_fx::display_name(fx),
                "on": fx.enabled,
                "latency": latency,
            });
            let target = FxTarget {
                context: context.clone(),
                fx: FxRef::Guid(fx.guid.clone()),
            };
            let all = Effects::parameters(daw, project.clone(), target);
            row["parameters"] = json!(all.len());
            if listed {
                let lines: Vec<String> = all
                    .iter()
                    .filter(|p| filter.is_none_or(|f| p.name.to_lowercase().contains(f)))
                    .map(param_line)
                    .collect();
                row["params"] = json!(lines);
            }
            if let Some(signal) = signal {
                row["signal"] = signal;
            }
            row
        })
        .collect();
    (rows, latencies.iter().sum())
}

/// A lane's name: the envelope's, or its FX's place and parameter's.
fn lane_name(e: &daw_proto::automation::Envelope, places: &HashMap<&str, usize>) -> String {
    match (&e.fx_guid, e.param_index) {
        (Some(fx), Some(_)) => format!(
            "fx#{} {}",
            places.get(fx.as_str()).map_or(0, |p| p + 1),
            e.name
        ),
        _ => e.name.clone(),
    }
}

/// A track's automation lanes with points in `window`.
fn automation_json(
    daw: &Standalone,
    project: &ProjectContext,
    track: &str,
    window: Window,
) -> Vec<Value> {
    let chain = Effects::list(
        daw,
        project.clone(),
        FxChainContext::Track(track.to_owned()),
    );
    let places: HashMap<&str, usize> = chain
        .iter()
        .enumerate()
        .map(|(i, f)| (f.guid.as_str(), i))
        .collect();
    Automation::envelopes(daw, project.clone(), TrackRef::Guid(track.to_owned()))
        .iter()
        .filter_map(|e| {
            let what = match (&e.fx_guid, e.param_index) {
                (Some(fx_guid), Some(param_index)) => EnvelopeRef::FxParam {
                    fx_guid: fx_guid.clone(),
                    param_index,
                },
                _ if e.envelope_type == EnvelopeType::FxParam => return None,
                _ => EnvelopeRef::Type(e.envelope_type),
            };
            let at = EnvelopeLocation::new(TrackRef::Guid(track.to_owned()), what);
            let points = match window {
                Some((from, to)) => Automation::points_in_range(
                    daw,
                    project.clone(),
                    at,
                    TimeRangeParams {
                        start: PositionInSeconds::from_seconds(from),
                        end: PositionInSeconds::from_seconds(to),
                    },
                ),
                None => Automation::points(daw, project.clone(), at),
            };
            if points.is_empty() {
                return None;
            }
            Some(json!({
                "lane": lane_name(e, &places),
                "mode": crate::fx_actions::mode_word(e.automation_mode),
                "points": points
                    .iter()
                    .map(|p| json!([
                        (p.time.as_seconds() * 1e4).round() / 1e4,
                        (p.value * 1e4).round() / 1e4,
                        crate::fx_actions::shape_word(p.shape),
                    ]))
                    .collect::<Vec<_>>(),
            }))
        })
        .collect()
}

/// A track's meter now, dBFS: `[L, R]` peak and held.
fn meter_json(meters: &daw_standalone::metering::Meters, index: u32) -> Value {
    let Some(cell) = meters.cell(index as usize) else {
        return Value::Null;
    };
    let dbfs = |x: f32| r2(daw_standalone::metering::linear_to_db(x));
    json!({
        "peak": [dbfs(cell.peak(0)), dbfs(cell.peak(1))],
        "hold": [dbfs(cell.hold(0)), dbfs(cell.hold(1))],
    })
}

/// `MIX_STATE`.
///
/// # Errors
/// The track or the span is not one the song has.
fn state(daw: &Standalone, project: &ProjectContext, a: &StateArg) -> Result<Value, String> {
    let span = tracing::info_span!(
        "MIX_STATE",
        mix.tracks = tracing::field::Empty,
        mix.fx = tracing::field::Empty,
        mix.lanes = tracing::field::Empty,
        mix.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let root = resolve(daw, project, &a.track)?;
    let window: Window = match (&a.from, &a.to) {
        (None, None) => None,
        (from, to) => Some((
            from.as_deref()
                .map_or(Ok(0.0), |f| crate::fx_actions::moment(daw, project, f))?,
            to.as_deref()
                .map_or(Ok(f64::MAX), |t| crate::fx_actions::moment(daw, project, t))?,
        )),
    };
    let params = match a.params {
        Some(true) => Params::All,
        Some(false) => Params::None,
        None => Params::On,
    };
    let filter = a.filter.as_ref().map(|f| f.to_lowercase());
    let all = Tracks::all(daw, project.clone());
    let path = paths(&all);
    let path_of = |g: &str| path.get(g).cloned().unwrap_or_else(|| g.to_owned());
    let sends: HashMap<String, Vec<daw_proto::TrackRoute>> = all
        .iter()
        .map(|t| {
            (
                t.guid.clone(),
                Routing::sends(daw, project.clone(), TrackRef::Guid(t.guid.clone())),
            )
        })
        .collect();
    let chain: HashMap<&str, u32> = all
        .iter()
        .map(|t| {
            let fx = Effects::list(daw, project.clone(), FxChainContext::Track(t.guid.clone()));
            (t.guid.as_str(), fx_latencies(daw, &fx).iter().sum())
        })
        .collect();
    let order: Vec<&str> = all.iter().map(|t| t.guid.as_str()).collect();
    let latency = output_latency(&order, &feeders(&all, &sends), &chain);
    let meters = daw.meters();
    let (mut fx_count, mut lane_count) = (0usize, 0usize);
    let rows: Vec<Value> = members(&all, &root, a.depth)
        .iter()
        .map(|m| {
            let t = m.track;
            let p = path_of(&t.guid);
            let (fx, chain_latency) =
                fx_json(daw, project, (&t.guid, &p), (params, filter.as_deref()));
            let automation = automation_json(daw, project, &t.guid, window);
            fx_count += fx.len();
            lane_count += automation.len();
            let out_sends: Vec<Value> = sends
                .get(&t.guid)
                .into_iter()
                .flatten()
                .map(|r| {
                    route_json(
                        r,
                        &r.dest_track_guid
                            .as_deref()
                            .map_or_else(String::new, path_of),
                    )
                })
                .collect();
            let receives: Vec<Value> =
                Routing::receives(daw, project.clone(), TrackRef::Guid(t.guid.clone()))
                    .iter()
                    .map(|r| {
                        let other = if r.source_track_guid == t.guid {
                            r.dest_track_guid.as_deref().unwrap_or_default()
                        } else {
                            r.source_track_guid.as_str()
                        };
                        route_json(r, &path_of(other))
                    })
                    .collect();
            json!({
                "path": p,
                "level": m.level,
                "kind": if t.is_folder { "folder" } else { "track" },
                "volume_db": r2(db(t.volume)),
                "pan": r2(t.pan),
                "mute": t.muted,
                "solo": t.soloed,
                "phase": t.phase_inverted,
                "to_parent": t.parent_send,
                "sends": out_sends,
                "receives": receives,
                "chain_latency": chain_latency,
                "latency": latency.get(&t.guid).copied().unwrap_or(0),
                "meter": meter_json(&meters, t.index),
                "fx": fx,
                "automation": automation,
            })
        })
        .collect();
    let ms = began.elapsed().as_secs_f64() * 1e3;
    span.record("mix.tracks", rows.len());
    span.record("mix.fx", fx_count);
    span.record("mix.lanes", lane_count);
    span.record("mix.ms", ms);
    Ok(json!({
        "track": path_of(&root),
        "from": window.map(|w| w.0),
        "to": window.map(|w| w.1).filter(|t| *t < f64::MAX),
        "tracks": rows,
        "ms": r2(ms),
    }))
}

pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register_with(&STATE, Effect::ReadOnly, |_: &ActionCaller, a: StateArg| {
        let (daw, project) = song()?;
        state(daw, &project, &a).map(|v| RawJson::from(v.to_string()))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_latency_is_the_latest_input_plus_the_chain() {
        // Bus <- A (64), B (0) ; Bus chain 10 ; A fed by C (5)
        let order = ["bus", "a", "b", "c"];
        let inputs: HashMap<&str, Vec<&str>> = [("bus", vec!["a", "b"]), ("a", vec!["c"])]
            .into_iter()
            .collect();
        let chain: HashMap<&str, u32> = [("bus", 10), ("a", 64), ("b", 0), ("c", 5)]
            .into_iter()
            .collect();
        let out = output_latency(&order, &inputs, &chain);
        assert_eq!(out["c"], 5);
        assert_eq!(out["a"], 69);
        assert_eq!(out["b"], 0);
        assert_eq!(out["bus"], 79);
    }

    #[test]
    fn a_loop_does_not_recurse_forever() {
        let order = ["a", "b"];
        let inputs: HashMap<&str, Vec<&str>> =
            [("a", vec!["b"]), ("b", vec!["a"])].into_iter().collect();
        let chain: HashMap<&str, u32> = [("a", 1), ("b", 2)].into_iter().collect();
        let out = output_latency(&order, &inputs, &chain);
        assert!(out["a"] >= 1 && out["b"] >= 2);
    }
}
