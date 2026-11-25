//! Signal in Studio, read and driven by track through its
//! `com.fasttrackstudio.signal/1`: the chain as data (`SIGNAL_CHAIN`, its
//! `signal.chain/1` read again only when the plugin's revision moved),
//! any Signal action (`SIGNAL_RUN`), every port of the song's Signal
//! sessions (`SIGNAL_SESSION`), and the mixer's live feed — each Signal
//! FX's chain revision and meter values, read beside the render without
//! blocking it (`SIGNAL_FEED`, [`feed`], the [`feed_hub`] stream).
//!
//! A meter is a modulation source the plugin publishes
//! (`mod_source_value`, lock-free on the plugin's side): read here
//! without the plugin's lock, through a client kept from the instance and
//! trusted only while the engine's plugin map has not changed since
//! (`Standalone::with_plugin_map`'s generation).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use architect::action::{ActionCaller, ActionError, ActionList, ActionMeta, Effect, RawJson};
use daw::service::{Effects, FxChainContext, ProjectContext, Tracks};
use daw::standalone::Standalone;
use daw::standalone::clap_ext::host::SignalPluginClient;
use facet::Facet;
use serde_json::{Value, json};

use crate::actions::{meta, resolve, song};

static CHAIN: ActionMeta = meta(
    "SIGNAL_CHAIN",
    "signal_chain",
    "Signal chain on a track",
    "The Signal chain on `track` (a GUID, a path like `Drums/Kick/Sum/In`, any tail of it, or a name), as its Signal instance gives it: `{track, track_guid, fx: {guid, place, name}, revision, cached, chain}`. `chain` is the plugin's `signal.chain/1` — `schema`, `revision`, `session` (the shared session it is in, or null), `port` (its track in the kit), `mode` (`per-track` or `kit`), `midi` (what its note input plays; `publishes_hits`), `phases` (each `{id, name, bypass, makeup_db, blocks}`; a block `{id (stable: rescue.gate), kind (eq, comp, gate, saturate, reverb, delay, nam, sampler, trigger, clap…), name, bypass, backend (linked or clap), status (ok, unavailable, error), key ({from, via: hits|midi|self} or null), params, meters}`; a param `{id, clap_id (the FX parameter slot, FX_PARAM_SET sets it), name, value, normalized, text, unit, min, max, default, choices}`; a meter `{id, name, unit, index}` — its live value is in SIGNAL_FEED) and `macros`. The chain is read from the plugin when its revision moved since the last read, or that read is older than 250 ms (a parameter value moves without the revision); otherwise it is the kept one (`cached: true`). `fx` (a name, or `#place`) picks one when the track holds more than one Signal instance. Structural changes (bypass a block or a phase, add, remove, move a block) are SIGNAL_RUN's (`SIGNAL_SET`, `SIGNAL_BLOCK_ADD`, …); a parameter's value is FX_PARAM_SET on its `clap_id`.",
    "FX",
    "",
);

static RUN: ActionMeta = meta(
    "SIGNAL_RUN",
    "signal_run",
    "Run a Signal action",
    "Run one of Signal's own actions on the Signal instance on `track` (as SIGNAL_CHAIN finds it; `fx` picks one of several): `action` its id (`SIGNAL_SET`, `SIGNAL_BLOCK_ADD`, `SIGNAL_BLOCK_REMOVE`, `SIGNAL_BLOCK_MOVE`, `SIGNAL_PRESET_LOAD`, `SIGNAL_BLOCK_DEV`, … — `SIGNAL_ACTIONS` is not one: `action: \"list\"` gives the plugin's own list), `args` its arguments (an object, or its JSON as a string). Examples: `{\"track\": \"Kick/Sum/In\", \"action\": \"SIGNAL_SET\", \"args\": {\"block\": \"rescue.gate\", \"bypass\": true}}`; `{\"action\": \"SIGNAL_SET\", \"args\": {\"phase\": \"rescue\", \"makeup_db\": 0.5}}`; `{\"action\": \"SIGNAL_BLOCK_ADD\", \"args\": {\"phase\": \"tone\", \"kind\": \"eq\", \"after\": \"tone.comp\"}}`. Answers `{track, track_guid, fx_guid, action, result}`, `result` the plugin's own answer (a structural change answers the new signal.chain/1). Runs on the plugin's main thread; its track's audio waits for it.",
    "FX",
    "",
);

static SESSION: ActionMeta = meta(
    "SIGNAL_SESSION",
    "signal_session",
    "Every Signal port",
    "Every Signal instance on the song, by the Signal session it is in: `{sessions: [{session (its id, or null for instances in none), ports: [{track, track_guid, fx_guid, place, revision, chain}]}], ports}` — each port's `chain` its signal.chain/1, as SIGNAL_CHAIN gives it (read again only where its revision moved). What a Signal editor showing the whole kit reads.",
    "FX",
    "",
);

static FEED: ActionMeta = meta(
    "SIGNAL_FEED",
    "signal_feed",
    "Signal's live values",
    "The mixer's live feed of Signal, cheap enough to read ten times a second: `{at_ms, fx: [{track, track_guid, fx_guid, place, name, revision, values_revision, meters: [{id, name, unit, index, value}]}]}`, one row per Signal instance on the song. `revision` is its chain's: when it moved, read SIGNAL_CHAIN again; `values_revision` (the chain's, null from a Signal build without it; at most 250 ms old) moves when a parameter value does: read the values again. A meter's `value` (in its `unit`: dB, %, Hz) is what the block published after its last processed block, read without holding up the audio. The same rows stream to a window in this process (`session_engine::clap_host::signal::feed_hub`).",
    "FX",
    "",
);

/// Which track, and which of its Signal instances.
#[derive(Facet)]
struct ChainArg {
    track: String,
    fx: Option<String>,
}

/// What SIGNAL_RUN takes, for its schema (it reads the JSON itself: `args`
/// may be an object).
#[derive(Facet)]
struct RunDoc {
    track: String,
    action: String,
    /// Its arguments: an object, or its JSON as a string.
    args: Option<String>,
    fx: Option<String>,
}

/// One meter's value, as the feed carries it.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct MeterValue {
    pub id: String,
    pub name: String,
    pub unit: String,
    /// Its index among the plugin's modulation sources.
    pub index: u32,
    pub value: f64,
}

/// One Signal instance, as the feed carries it.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct FeedRow {
    /// The track's path.
    pub track: String,
    pub track_guid: String,
    pub fx_guid: String,
    /// Its place in the chain (`#3`).
    pub place: String,
    pub name: String,
    /// Its chain's revision: SIGNAL_CHAIN again when it moved.
    pub revision: u64,
    /// Its parameter values' revision (the chain's `values_revision`):
    /// read the values again when it moved. `None` from a Signal build
    /// that does not send it. As of the chain's last read, at most
    /// [`VALUES_FRESH`] ago, until signal/1 has a thread-safe
    /// `values_revision()`.
    pub values_revision: Option<u64>,
    pub meters: Vec<MeterValue>,
}

/// The feed at one moment.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct Feed {
    /// Milliseconds since the engine's start, when it was read.
    pub at_ms: f64,
    pub fx: Vec<FeedRow>,
}

/// A Signal instance on the song.
#[derive(Clone)]
struct Port {
    track: String,
    track_guid: String,
    fx_guid: String,
    place: String,
    name: String,
}

/// What is kept of an instance between reads.
struct Kept {
    /// `None`: the plugin has no signal/1.
    client: Option<SignalPluginClient>,
    /// Its last chain read, at its revision.
    chain: Option<(u64, Arc<Value>, web_time::Instant)>,
    /// Its meters, as of the chain revision they were read at.
    meters: Option<(u64, Vec<daw::standalone::clap_ext::ModSourceInfo>)>,
}

#[derive(Default)]
struct Cache {
    /// The plugin map's generation the clients are from.
    generation: u64,
    /// The engine they are from (a process runs one; a test may run more).
    engine: usize,
    by_fx: HashMap<String, Kept>,
    /// The song's Signal instances, as last found, and when.
    ports: Option<(u64, web_time::Instant, Vec<Port>)>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

fn with_cache<R>(f: impl FnOnce(&mut Cache) -> R) -> R {
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    f(cache.get_or_insert_with(Cache::default))
}

/// The cache, its clients dropped when the plugin map moved on.
fn current(cache: &mut Cache, (engine, generation): (usize, u64)) {
    if cache.generation != generation || cache.engine != engine {
        cache.generation = generation;
        cache.engine = engine;
        cache.by_fx.clear();
        cache.ports = None;
    }
}

/// The signal/1 client of `fx_guid`'s plugin, kept for the generation.
fn client_of(daw: &Standalone, fx_guid: &str) -> Option<SignalPluginClient> {
    let generation = daw.with_plugin_map(|m| m.generation());
    if let Some(kept) = with_cache(|c| {
        current(c, (std::ptr::from_ref(daw).addr(), generation));
        c.by_fx.get(fx_guid).map(|k| k.client)
    }) {
        return kept;
    }
    let client = daw.with_plugin_instance(fx_guid, |p| p.signal())?;
    with_cache(|c| {
        if c.generation == generation {
            c.by_fx.insert(
                fx_guid.to_owned(),
                Kept {
                    client,
                    chain: None,
                    meters: None,
                },
            );
        }
    });
    client
}

/// A Signal instance's revision, read without its lock: `None` when the
/// plugin map moved since the client was kept (read it again).
fn revision_of(daw: &Standalone, fx_guid: &str) -> Option<u64> {
    let client = client_of(daw, fx_guid)?;
    let generation = with_cache(|c| c.generation);
    daw.with_plugin_map(|m| {
        (m.generation() == generation && m.contains_key(fx_guid)).then(|| client.revision())
    })
}

/// The Signal instances on `project`'s tracks, found again when the plugin
/// map moved or a second has passed (an FX moved keeps its instance).
fn ports(daw: &Standalone, project: &ProjectContext) -> Vec<Port> {
    let generation = daw.with_plugin_map(|m| m.generation());
    let fresh = with_cache(|c| {
        current(c, (std::ptr::from_ref(daw).addr(), generation));
        c.ports
            .as_ref()
            .filter(|(g, at, _)| *g == generation && at.elapsed().as_secs_f64() < 1.0)
            .map(|(_, _, ports)| ports.clone())
    });
    if let Some(ports) = fresh {
        return ports;
    }
    let tracks = Tracks::all(daw, project.clone());
    let paths = crate::mix_sends::paths(&tracks);
    let mut found = Vec::new();
    for track in &tracks {
        let chain = Effects::list(
            daw,
            project.clone(),
            FxChainContext::Track(track.guid.clone()),
        );
        for (i, fx) in chain.iter().enumerate() {
            if client_of(daw, &fx.guid).is_some() {
                found.push(Port {
                    track: paths
                        .get(&track.guid)
                        .cloned()
                        .unwrap_or_else(|| track.name.clone()),
                    track_guid: track.guid.clone(),
                    fx_guid: fx.guid.clone(),
                    place: format!("#{}", i + 1),
                    name: crate::clap_fx::display_name(fx),
                });
            }
        }
    }
    with_cache(|c| {
        if c.generation == generation {
            c.ports = Some((generation, web_time::Instant::now(), found.clone()));
        }
    });
    found
}

/// The Signal instance on `track` (`fx` picks one of several).
fn port_on(
    daw: &Standalone,
    project: &ProjectContext,
    track: &str,
    fx: Option<&str>,
) -> Result<Port, String> {
    let guid = resolve(daw, project, track)?;
    let on: Vec<Port> = ports(daw, project)
        .into_iter()
        .filter(|p| p.track_guid == guid)
        .collect();
    let which = fx.map(str::trim).filter(|w| !w.is_empty());
    let picked = match which {
        Some(w) => on
            .iter()
            .find(|p| {
                p.place == w || p.fx_guid == w || p.name.to_lowercase().contains(&w.to_lowercase())
            })
            .cloned(),
        None => on.first().cloned(),
    };
    picked.ok_or_else(|| match (which, on.is_empty()) {
        (_, true) => format!("no Signal instance (no signal/1) on {track}"),
        (Some(w), false) => format!(
            "no Signal instance {w:?} on {track} — it has {}",
            on.iter()
                .map(|p| format!("{} {}", p.place, p.name))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        (None, false) => unreachable!("a first exists"),
    })
}

/// `port`'s chain, read from the plugin only when its revision moved; and
/// whether it was the kept one.
fn chain_of_port(daw: &Standalone, port: &Port) -> Result<(u64, Arc<Value>, bool), String> {
    let revision = revision_of(daw, &port.fx_guid);
    if let Some(rev) = revision
        && let Some(kept) = with_cache(|c| {
            c.by_fx
                .get(&port.fx_guid)
                .and_then(|k| k.chain.clone())
                .filter(|(r, _, at)| *r == rev && at.elapsed() < VALUES_FRESH)
        })
    {
        return Ok((kept.0, kept.1, true));
    }
    let read = daw
        .with_plugin_instance(&port.fx_guid, |p| {
            super::plugin_call(|| {
                // Known before it runs: what it writes through
                // host-automation/1 names a plugin Studio knows.
                if super::automation::caller_of(&port.fx_guid).is_none() {
                    super::register_plugin(daw, &port.fx_guid, None, p);
                }
                let s = p.signal()?;
                Some((s.revision(), s.chain()))
            })
        })
        .flatten()
        .ok_or_else(|| format!("{} on {} has no signal/1", port.name, port.track))?;
    let (rev, json) = (read.0, read.1.map_err(|e| e.to_string())?);
    let chain: Arc<Value> = Arc::new(
        serde_json::from_str(&json)
            .map_err(|e| format!("{}'s chain is not JSON: {e}", port.name))?,
    );
    with_cache(|c| {
        if let Some(k) = c.by_fx.get_mut(&port.fx_guid) {
            k.chain = Some((rev, Arc::clone(&chain), web_time::Instant::now()));
        }
    });
    Ok((rev, chain, false))
}

/// How long a chain read stands for the plugin's parameter values while
/// its revision holds: a value changes without moving the revision (the
/// contract keeps them apart), so a chain is read again after this, and
/// the feed's `values_revision` is at most this old.
pub const VALUES_FRESH: std::time::Duration = std::time::Duration::from_millis(250);

/// SIGNAL_CHAIN.
///
/// # Errors
/// No such track, no Signal instance on it, or its chain unreadable.
pub fn chain(
    daw: &Standalone,
    project: &ProjectContext,
    track: &str,
    fx: Option<&str>,
) -> Result<Value, String> {
    let span = tracing::info_span!(
        "SIGNAL_CHAIN",
        signal.track = track,
        signal.revision = tracing::field::Empty,
        signal.cached = tracing::field::Empty,
    );
    let _in = span.enter();
    let port = port_on(daw, project, track, fx)?;
    let (revision, chain, cached) = chain_of_port(daw, &port)?;
    span.record("signal.revision", revision);
    span.record("signal.cached", cached);
    Ok(json!({
        "track": port.track,
        "track_guid": port.track_guid,
        "fx": { "guid": port.fx_guid, "place": port.place, "name": port.name },
        "revision": revision,
        "cached": cached,
        "chain": *chain,
    }))
}

/// SIGNAL_RUN: Signal's own `action` with `args` (JSON) on `track`'s
/// instance. `list` gives the plugin's list of actions.
///
/// # Errors
/// No instance, or the plugin's own refusal.
pub fn run(
    daw: &Standalone,
    project: &ProjectContext,
    (track, fx): (&str, Option<&str>),
    action: &str,
    args: &str,
) -> Result<Value, String> {
    let span = tracing::info_span!(
        "SIGNAL_RUN",
        signal.track = track,
        signal.action = action,
        signal.outcome = tracing::field::Empty,
    );
    let _in = span.enter();
    let port = port_on(daw, project, track, fx)?;
    let answer = daw
        .with_plugin_instance(&port.fx_guid, |p| {
            super::plugin_call(|| {
                // Known before it runs: what it writes through
                // host-automation/1 names a plugin Studio knows.
                if super::automation::caller_of(&port.fx_guid).is_none() {
                    super::register_plugin(
                        daw,
                        &port.fx_guid,
                        Some((project.clone(), port.track_guid.clone())),
                        p,
                    );
                }
                let s = p.signal()?;
                Some(if action.eq_ignore_ascii_case("list") {
                    s.list_actions()
                } else {
                    s.run_action(action, args)
                })
            })
        })
        .flatten()
        .ok_or_else(|| format!("{} on {} has no signal/1", port.name, port.track))?;
    let result = match answer {
        Ok(out) => {
            span.record("signal.outcome", "ok");
            out
        }
        Err(why) => {
            span.record("signal.outcome", "refused");
            tracing::warn!(signal.action = action, signal.why = %why, "SIGNAL_RUN: the plugin refused");
            return Err(format!("{action} on {}: {why}", port.track));
        }
    };
    let result = serde_json::from_str::<Value>(&result).unwrap_or(Value::String(result));
    Ok(json!({
        "track": port.track,
        "track_guid": port.track_guid,
        "fx_guid": port.fx_guid,
        "action": action,
        "result": result,
    }))
}

/// SIGNAL_SESSION: every port, by session.
#[must_use]
pub fn session(daw: &Standalone, project: &ProjectContext) -> Value {
    let mut by_session: Vec<(Value, Vec<Value>)> = Vec::new();
    let mut count = 0usize;
    for port in ports(daw, project) {
        let Ok((revision, chain, _)) = chain_of_port(daw, &port) else {
            continue;
        };
        let session = chain.get("session").cloned().unwrap_or(Value::Null);
        let row = json!({
            "track": port.track,
            "track_guid": port.track_guid,
            "fx_guid": port.fx_guid,
            "place": port.place,
            "revision": revision,
            "chain": *chain,
        });
        count += 1;
        match by_session.iter_mut().find(|(s, _)| *s == session) {
            Some((_, rows)) => rows.push(row),
            None => by_session.push((session, vec![row])),
        }
    }
    let sessions: Vec<Value> = by_session
        .into_iter()
        .map(|(session, ports)| json!({ "session": session, "ports": ports }))
        .collect();
    json!({ "sessions": sessions, "ports": count })
}

/// When the engine started, for [`Feed::at_ms`].
fn epoch() -> web_time::Instant {
    static EPOCH: OnceLock<web_time::Instant> = OnceLock::new();
    *EPOCH.get_or_init(web_time::Instant::now)
}

/// A port's meters (read from the plugin again when its chain revision
/// moved), with the client that reads their values.
fn meters_of(
    daw: &Standalone,
    port: &Port,
    revision: u64,
) -> Vec<daw::standalone::clap_ext::ModSourceInfo> {
    if let Some(kept) = with_cache(|c| {
        c.by_fx
            .get(&port.fx_guid)
            .and_then(|k| k.meters.clone())
            .filter(|(r, _)| *r == revision)
    }) {
        return kept.1;
    }
    let infos = daw
        .with_plugin_instance(&port.fx_guid, |p| {
            super::plugin_call(|| p.signal().map(|s| s.mod_sources()))
        })
        .flatten()
        .unwrap_or_default();
    with_cache(|c| {
        if let Some(k) = c.by_fx.get_mut(&port.fx_guid) {
            k.meters = Some((revision, infos.clone()));
        }
    });
    infos
}

/// The feed now: every Signal instance's chain revision and meter values,
/// the values read beside the render (no plugin's lock taken for them).
#[must_use]
pub fn feed(daw: &Standalone, project: &ProjectContext) -> Feed {
    let ports = ports(daw, project);
    let mut rows = Vec::with_capacity(ports.len());
    for port in ports {
        let Some(revision) = revision_of(daw, &port.fx_guid) else {
            continue;
        };
        let infos = meters_of(daw, &port, revision);
        // The values' revision is the chain's: read again at most every
        // VALUES_FRESH (kept between, while the chain's revision holds).
        let values_revision = chain_of_port(daw, &port)
            .ok()
            .and_then(|(_, chain, _)| chain.get("values_revision").and_then(Value::as_u64));
        let Some(client) = client_of(daw, &port.fx_guid) else {
            continue;
        };
        let generation = with_cache(|c| c.generation);
        let values: Option<Vec<f64>> = daw.with_plugin_map(|m| {
            (m.generation() == generation && m.contains_key(&port.fx_guid)).then(|| {
                (0..infos.len())
                    .map(|i| {
                        u32::try_from(i)
                            .ok()
                            .and_then(|i| client.mod_source_value(i))
                            .unwrap_or(0.0)
                    })
                    .collect()
            })
        });
        let Some(values) = values else {
            continue;
        };
        let meters = infos
            .into_iter()
            .zip(values)
            .enumerate()
            .map(|(i, (info, value))| MeterValue {
                id: info.id,
                name: info.name,
                unit: info.unit,
                index: u32::try_from(i).unwrap_or(u32::MAX),
                value,
            })
            .collect();
        rows.push(FeedRow {
            track: port.track,
            track_guid: port.track_guid,
            fx_guid: port.fx_guid,
            place: port.place,
            name: port.name,
            revision,
            values_revision,
            meters,
        });
    }
    Feed {
        at_ms: epoch().elapsed().as_secs_f64() * 1e3,
        fx: rows,
    }
}

/// The feed as a stream, for a window in this process (the mixer):
/// [`Feed`] about ten times a second while anyone subscribes
/// (`feed_hub().subscribe(n)`), the newest kept when a reader falls
/// behind.
#[must_use]
pub fn feed_hub() -> &'static architect::PubSub<Feed> {
    static HUB: OnceLock<architect::PubSub<Feed>> = OnceLock::new();
    HUB.get_or_init(|| architect::PubSub::sliding(4))
}

/// How often the hub is fed.
const FEED_EVERY: std::time::Duration = std::time::Duration::from_millis(100);

/// Feed the hub, at most every [`FEED_EVERY`], when anyone listens.
pub(super) fn publish_feed(daw: &Standalone) {
    static LAST: Mutex<Option<web_time::Instant>> = Mutex::new(None);
    let hub = feed_hub();
    if hub.subscriber_count() == 0 {
        return;
    }
    {
        let mut last = LAST.lock().unwrap_or_else(PoisonError::into_inner);
        if last.is_some_and(|at| at.elapsed() < FEED_EVERY) {
            return;
        }
        *last = Some(web_time::Instant::now());
    }
    let Ok((engine, project)) = song() else {
        return;
    };
    if std::ptr::eq(engine, daw) {
        hub.publish(feed(daw, &project));
    }
}

/// The song and track `fx_guid` is on (the song open now).
pub(super) fn chain_of(daw: &Standalone, fx_guid: &str) -> Option<(String, daw_proto::Track)> {
    let (engine, project) = song().ok()?;
    if !std::ptr::eq(engine, daw) {
        return None;
    }
    let guid = crate::fx_actions::project_guid(&project).ok()?;
    Tracks::all(daw, project.clone())
        .into_iter()
        .find(|t| {
            Effects::list(daw, project.clone(), FxChainContext::Track(t.guid.clone()))
                .iter()
                .any(|fx| fx.guid == fx_guid)
        })
        .map(|t| (guid, t))
}

/// What a failed Signal action says to the Action List.
fn failed(id: &str, message: String) -> ActionError {
    ActionError::Failed {
        id: id.to_owned(),
        message,
    }
}

/// SIGNAL_RUN's arguments, read from its JSON.
fn run_args(raw: &[u8]) -> Result<(String, String, Option<String>, String), ActionError> {
    let bad = |message: String| ActionError::BadArgs {
        id: RUN.id.to_owned(),
        message,
        expected: Some(architect::schema::json_schema(RunDoc::SHAPE)),
    };
    let v: Value = serde_json::from_slice(if raw.is_empty() { b"{}" } else { raw })
        .map_err(|e| bad(e.to_string()))?;
    let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
    let track = text("track").ok_or_else(|| bad("`track` is missing".into()))?;
    let action = text("action").ok_or_else(|| bad("`action` is missing".into()))?;
    let args = match v.get("args") {
        None | Some(Value::Null) => "{}".to_owned(),
        Some(Value::String(s)) => {
            serde_json::from_str::<Value>(s)
                .map_err(|e| bad(format!("`args` is not JSON: {e}")))?;
            s.clone()
        }
        Some(other) => other.to_string(),
    };
    Ok((track, action, text("fx"), args))
}

pub fn register(list: &ActionList) -> Result<(), ActionError> {
    list.register_with(&CHAIN, Effect::ReadOnly, |_: &ActionCaller, a: ChainArg| {
        let (daw, project) = song()?;
        chain(daw, &project, &a.track, a.fx.as_deref()).map(|v| RawJson::from(v.to_string()))
    })?;
    list.register_dynamic(
        &RUN,
        Effect::Mutating,
        Some(RunDoc::SHAPE),
        None,
        Arc::new(|raw: &[u8]| {
            let (track, action, fx, args) = run_args(raw)?;
            let (daw, project) = song().map_err(|m| failed(RUN.id, m))?;
            run(daw, &project, (&track, fx.as_deref()), &action, &args)
                .map(|v| v.to_string().into_bytes())
                .map_err(|m| failed(RUN.id, m))
        }),
    )?;
    list.register_with(&SESSION, Effect::ReadOnly, |_: &ActionCaller, (): ()| {
        let (daw, project) = song()?;
        Ok::<_, String>(RawJson::from(session(daw, &project).to_string()))
    })?;
    list.register(&FEED, Effect::ReadOnly, |(): ()| {
        let (daw, project) = song()?;
        Ok::<_, String>(feed(daw, &project))
    })?;
    Ok(())
}
