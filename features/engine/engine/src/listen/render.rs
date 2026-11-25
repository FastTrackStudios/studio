//! A partial render: only the tracks that feed the points listened to,
//! offline, and nothing rendered at all when nothing they hear changed.
//!
//! MIX_MEASURE renders the whole song — every one of the kit's 55 Signal
//! instances copied, then run — to learn about one track. A listen asks
//! for a few points (a track's input, an FX's output, a fader), so it
//! renders what reaches them and nothing else: each point's track, every
//! track inside it, and whatever sends into any of those (daw's
//! `Offline::only`; the rest of the song renders empty). A kit piece's
//! trigger comes along whenever one of its mics does: the other tracks'
//! plugins key from it, though nothing of it is heard.
//!
//! Kept between calls:
//!
//! - **What each track sounded like**, at each point a listen read and
//!   after its fader, with a key: the track's own fingerprint as a render
//!   reads it (media, fader, FX switches and values, automation), its
//!   plugins' states, and the keys of everything feeding it. A listen
//!   whose points all have a recording under their current key, covering
//!   its span, renders nothing. Otherwise a track whose key is unchanged
//!   is played back in place (`Offline::fed`). A Signal instance's output
//!   depends only on its own input and the hits of the pieces its keys
//!   follow (`key.from` in its `signal.chain/1`), so a rendered Signal
//!   track renders with the tracks it keys from (they publish those hits
//!   as they run) and nothing else of its session; a track's key carries
//!   its key sources' keys. When what a track keys from can't be read,
//!   everything the listen reaches renders.
//! - **The plugins' copies**, each with its live instance's state
//!   fingerprint: a copy whose plugin is unchanged is reused, reset (CLAP's
//!   `reset`: its buffers, voices and envelopes cleared, still activated),
//!   or deactivated where it cannot be; one that changed is made again.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use daw::service::ProjectContext;
use daw_proto::routing::SendMode;
use daw_standalone::audio_engine::render::ProjectRenderer;
use daw_standalone::audio_engine::render::offline::{
    Copying, Fed, Offline, Probe, ProbeAt, copy_plugin, hosted,
};
use daw_standalone::plugin::{PluginInstance, Plugins};
use daw_standalone::sync::Standalone;

/// The rate a listen renders at.
pub const RATE: u32 = crate::mix_measure::RATE;
/// Frames a block.
const BLOCK: usize = 1_024;
/// How long before `from` a render starts, seconds: what a compressor's
/// release and a reverb's tail need to be playing as they would.
pub const PREROLL: f64 = 1.0;
/// The longest span listened to at once, seconds.
const MOST: f64 = 600.0;
/// The most recordings kept (about 5 MB each for 13 s).
const KEPT_RECORDINGS: usize = 240;
/// The Signal session a chunk's copies share (`listen-<chunk>`): one of
/// their own, never the live kit's, and the same for every listen so a
/// reused copy and a fresh one meet in it.
const SESSION_TAG: &str = "listen";
/// A span is rendered in this many chunks at most, side by side, each on
/// copies of its own (in a Signal session of its own) from a preroll of
/// its own; a chunk is at least [`CHUNK_LEAST`] seconds.
const CHUNKS: u64 = 3;
const CHUNK_LEAST: f64 = 3.0;

/// A track whose plugins others key from (`from`, a kit piece's
/// trigger), and the tracks keyed (`by`, its piece's mics).
#[derive(Clone, Debug)]
pub struct Keyed {
    pub from: String,
    pub by: Vec<String>,
}

/// Which side of a Signal block a point is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    /// What arrives at it.
    Pre,
    /// What it puts out.
    Post,
}

/// A Signal instance's chain cut at one block: on the render's copy of
/// it, the block (for [`Side::Pre`]) and everything after it in its phase
/// bypassed, that phase's make-up at 0 and every later phase bypassed, so
/// the instance's output is the signal at that side of the block. The
/// live instance is never touched.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Cut {
    /// The block's id in `signal.chain/1` (`rescue.gate`).
    pub block: String,
    pub side: Side,
}

/// The `signal/1` actions that cut `chain` (its `signal.chain/1` JSON) at
/// `cut`: `(action, args)` pairs for SIGNAL_SET.
///
/// # Errors
/// The chain is not `signal.chain/1` with phases, or has no such block.
pub fn cut_actions(chain: &serde_json::Value, cut: &Cut) -> Result<Vec<(String, String)>, String> {
    use serde_json::json;
    let phases = chain["phases"]
        .as_array()
        .filter(|p| !p.is_empty())
        .ok_or("the instance has no phases (a kit-mode instance cannot be cut)")?;
    let found = phases.iter().enumerate().find_map(|(pi, p)| {
        p["blocks"]
            .as_array()?
            .iter()
            .position(|b| b["id"] == cut.block.as_str())
            .map(|bi| (pi, bi))
    });
    let (phase_at, block_at) = found.ok_or_else(|| {
        let ids: Vec<&str> = phases
            .iter()
            .filter_map(|p| p["blocks"].as_array())
            .flatten()
            .filter_map(|b| b["id"].as_str())
            .collect();
        format!(
            "no block {:?} — the chain has {}",
            cut.block,
            ids.join(", ")
        )
    })?;
    let set = |args: serde_json::Value| ("SIGNAL_SET".to_owned(), args.to_string());
    let mut out = Vec::new();
    let phase = &phases[phase_at];
    let first_cut = match cut.side {
        Side::Pre => block_at,
        Side::Post => block_at + 1,
    };
    for b in phase["blocks"]
        .as_array()
        .into_iter()
        .flatten()
        .skip(first_cut)
    {
        if let Some(id) = b["id"].as_str() {
            out.push(set(json!({ "block": id, "bypass": true })));
        }
    }
    if let Some(id) = phase["id"].as_str() {
        out.push(set(json!({ "phase": id, "makeup_db": 0.0 })));
    }
    for p in &phases[phase_at + 1..] {
        if let Some(id) = p["id"].as_str() {
            out.push(set(json!({ "phase": id, "bypass": true })));
        }
    }
    Ok(out)
}

/// Cut a copy's chain (see [`Cut`]) through its `signal/1`.
fn cut_copy(copy: &mut dyn PluginInstance, cut: &Cut) -> Result<(), String> {
    let signal = copy
        .signal()
        .ok_or("the FX has no signal/1: only a Signal instance has blocks")?;
    let chain =
        crate::clap_host::plugin_call(|| signal.chain()).map_err(|e| format!("its chain: {e}"))?;
    let chain: serde_json::Value =
        serde_json::from_str(&chain).map_err(|e| format!("its chain: {e}"))?;
    for (action, args) in cut_actions(&chain, cut)? {
        crate::clap_host::plugin_call(|| signal.run_action(&action, &args))
            .map_err(|e| format!("{action} {args}: {e}"))?;
    }
    Ok(())
}

/// What to render.
pub struct Ask<'a> {
    /// Seconds.
    pub from: f64,
    pub to: f64,
    /// The points to read, in the order the answer gives them.
    pub points: &'a [ProbeAt],
    /// Tracks soloed for this render alone (as the solo buttons would,
    /// inside what is rendered); `None` keeps the song's.
    pub solo: Option<HashSet<String>>,
    /// FX (by guid) rendered bypassed.
    pub bypass_fx: HashSet<String>,
    /// What runs whenever what it keys does.
    pub keyed: &'a [Keyed],
    /// Render everything again, playing nothing back (what is rendered is
    /// still kept for the next listen).
    pub fresh: bool,
    /// Signal instances (by FX guid) cut at a block for this render (a
    /// point at `block:<id>:pre|post`): fresh copies, cut, never kept.
    pub cuts: &'a HashMap<String, Cut>,
    /// Render in one piece, not in chunks side by side: slower, and what a
    /// whole render (MIX_MEASURE) would say to the sample. Chunks, each
    /// from a preroll of its own, measure within about 0.05 LU of it, and
    /// within a few tenths of a dB on peaks and crest.
    pub exact: bool,
}

/// How a listen was answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answered {
    /// From what earlier listens heard: nothing it hears has changed.
    Replayed,
    /// Rendered, the tracks whose key is unchanged played back.
    Partly,
    /// Everything it reaches rendered.
    Rendered,
}

impl Answered {
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Replayed => "replayed",
            Self::Partly => "partly",
            Self::Rendered => "rendered",
        }
    }
}

/// What a render heard.
pub struct Heard {
    /// Each point's signal, exactly `from`..`to`, interleaved stereo.
    pub signals: Vec<Vec<f32>>,
    /// How late each point was before it was lined up, samples.
    pub latencies: Vec<u32>,
    pub answered: Answered,
    /// The tracks rendered and played back, by guid, and the song's count.
    pub rendered: Vec<String>,
    pub replayed: Vec<String>,
    pub song_tracks: u32,
    /// Plugins run: copies made fresh, and copies reused.
    pub copied: u32,
    pub reused: u32,
    /// Plugins that would not be copied (rendered dry), and why.
    pub failures: Vec<String>,
    /// The chunks it was rendered in, side by side (0 when replayed).
    pub chunks: u32,
    pub plan_ms: f64,
    pub copy_ms: f64,
    pub prime_ms: f64,
    pub blocks_ms: f64,
    pub ms: f64,
}

/// A copy of a live plugin, and the fingerprint of the state it was made
/// from.
struct Warm {
    /// The fingerprint of the live instance's state the copy was made
    /// from: when it moves, the copy is made again (Signal loading a state
    /// into an instance that held another does not sound like a fresh one
    /// loading it).
    state: u64,
    /// The values the song held over its parameters when the copy last
    /// rendered: a value held then and let go since is still set in the
    /// copy, so when they differ the state is loaded into it again.
    held: u64,
    plugin: Box<dyn PluginInstance>,
}

/// One point of a track as a render heard it.
struct Recorded {
    key: u64,
    /// Rendered in one piece ([`Ask::exact`]).
    exact: bool,
    /// Render frames `start..end`.
    start: u64,
    end: u64,
    samples: Arc<[f32]>,
    latency: u32,
    /// When it was kept: the oldest goes first.
    stamp: u64,
}

impl Recorded {
    /// Whether it is what `key` sounds like over render frames
    /// `start..to` (and as late as it was past `to`).
    fn holds(&self, key: u64, (start, to): (u64, u64)) -> bool {
        self.key == key && self.start <= start && self.end >= to + u64::from(self.latency)
    }
}

/// A recording's address: a track, and where in it.
type Point = (String, Probe);

/// What listens keep between calls. Held for a whole render: two listens
/// never run at once (their copies share one Signal session).
#[derive(Default)]
struct Kept {
    /// Per chunk, by FX guid.
    warm: Vec<HashMap<String, Warm>>,
    /// By point and key: a point keeps what it sounded like before and
    /// after a change, so putting a change back replays the first.
    recordings: HashMap<(Point, u64), Recorded>,
    stamp: u64,
}

impl Kept {
    /// Keep a recording, the oldest let go past [`KEPT_RECORDINGS`].
    fn keep(&mut self, at: Point, mut r: Recorded) {
        self.stamp += 1;
        r.stamp = self.stamp;
        self.recordings.insert((at, r.key), r);
        while self.recordings.len() > KEPT_RECORDINGS {
            let Some(oldest) = self
                .recordings
                .iter()
                .min_by_key(|(_, r)| r.stamp)
                .map(|(p, _)| p.clone())
            else {
                break;
            };
            self.recordings.remove(&oldest);
        }
    }

    /// The recording of `at` under `key` over `frames`, if one is kept.
    fn holding(
        &self,
        at: &Point,
        (key, exact): (Option<u64>, bool),
        frames: (u64, u64),
    ) -> Option<&Recorded> {
        let key = key?;
        self.recordings
            .get(&(at.clone(), key))
            .filter(|r| r.holds(key, frames) && (r.exact || !exact))
    }
}

fn kept() -> &'static Mutex<Kept> {
    static KEPT: std::sync::OnceLock<Mutex<Kept>> = std::sync::OnceLock::new();
    KEPT.get_or_init(Mutex::default)
}

/// One send into a track: where from, and where it taps its source.
#[derive(Clone)]
pub struct Into {
    pub from: String,
    pub mode: SendMode,
}

/// The song's routing, as a listen walks it upstream.
pub struct Routes {
    /// Each folder's children that feed it.
    children: HashMap<String, Vec<String>>,
    /// Each track's incoming sends.
    sends: HashMap<String, Vec<Into>>,
}

impl Routes {
    /// The routing of `tracks` (each its guid, its folder, and whether it
    /// feeds that folder), `sends` each track's incoming sends.
    #[must_use]
    pub fn new<'a>(
        tracks: impl IntoIterator<Item = (&'a str, Option<&'a str>, bool)>,
        sends: HashMap<String, Vec<Into>>,
    ) -> Self {
        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        for (guid, parent, feeds) in tracks {
            if let (Some(p), true) = (parent, feeds) {
                children
                    .entry(p.to_owned())
                    .or_default()
                    .push(guid.to_owned());
            }
        }
        Self { children, sends }
    }

    /// What feeds `track` directly: its children, then what sends to it.
    fn feeders<'a>(&'a self, track: &str) -> impl Iterator<Item = &'a String> {
        self.children
            .get(track)
            .into_iter()
            .flatten()
            .chain(self.sends.get(track).into_iter().flatten().map(|s| &s.from))
    }

    /// Whether `track` sends anywhere from before its fader: a recording
    /// of its output could not stand in for those sends.
    fn taps_before_its_fader(&self, track: &str) -> bool {
        self.sends
            .values()
            .flatten()
            .any(|s| s.from == track && s.mode != SendMode::PostFader)
    }
}

/// The incoming sends of every track of song `project`.
#[must_use]
pub fn sends_into(
    daw: &Standalone,
    project: &ProjectContext,
    tracks: &[daw_proto::Track],
) -> HashMap<String, Vec<Into>> {
    use daw::service::{Routing, TrackRef};
    let mut into: HashMap<String, Vec<Into>> = HashMap::new();
    for t in tracks {
        for r in Routing::sends(daw, project.clone(), TrackRef::Guid(t.guid.clone())) {
            if let Some(to) = r.dest_track_guid {
                into.entry(to).or_default().push(Into {
                    from: t.guid.clone(),
                    mode: r.send_mode,
                });
            }
        }
    }
    into
}

/// Every track that reaches one of `roots`: each root and, walking
/// upstream, everything feeding it — except past a track `stop` says is
/// played back. The tracks reached, and of them those stopped at.
#[must_use]
pub fn reach(
    routes: &Routes,
    roots: impl IntoIterator<Item = String>,
    stop: impl Fn(&str) -> bool,
) -> (HashSet<String>, HashSet<String>) {
    let (mut seen, mut stopped) = (HashSet::new(), HashSet::new());
    let mut todo: Vec<String> = roots.into_iter().collect();
    while let Some(g) = todo.pop() {
        if !seen.insert(g.clone()) {
            continue;
        }
        if stop(&g) {
            stopped.insert(g);
            continue;
        }
        todo.extend(routes.feeders(&g).cloned());
    }
    (seen, stopped)
}

/// What a listen of `roots` reaches, its pieces' triggers with it: each
/// [`Keyed`] whose keyed tracks are reached comes along, with what feeds
/// it.
#[must_use]
pub fn reach_keyed(routes: &Routes, roots: &[String], keyed: &[Keyed]) -> HashSet<String> {
    let (mut all, _) = reach(routes, roots.iter().cloned(), |_| false);
    let extra: Vec<String> = keyed
        .iter()
        .filter(|k| !all.contains(&k.from) && k.by.iter().any(|b| all.contains(b)))
        .map(|k| k.from.clone())
        .collect();
    if !extra.is_empty() {
        all.extend(reach(routes, extra, |_| false).0);
    }
    all
}

/// Each reached track's key: its own fingerprint (`own`), then the keys
/// of what feeds it, in order — so a change anywhere upstream changes it.
fn keys(
    routes: &Routes,
    reached: &HashSet<String>,
    (own, sources): (&HashMap<String, u64>, &Sources),
) -> HashMap<String, u64> {
    fn key_of(
        g: &str,
        (routes, sources): (&Routes, &Sources),
        own: &HashMap<String, u64>,
        done: &mut HashMap<String, u64>,
        on_path: &mut HashSet<String>,
    ) -> u64 {
        if let Some(k) = done.get(g) {
            return *k;
        }
        let mut h = std::collections::hash_map::DefaultHasher::new();
        own.get(g).copied().unwrap_or(0).hash(&mut h);
        if on_path.insert(g.to_owned()) {
            for f in routes.feeders(g) {
                key_of(f, (routes, sources), own, done, on_path).hash(&mut h);
            }
            // What its blocks key from: a change there changes what it
            // plays, though none of its audio comes this way.
            for f in sources.get(g).into_iter().flatten().flatten() {
                key_of(f, (routes, sources), own, done, on_path).hash(&mut h);
            }
            on_path.remove(g);
        }
        let k = h.finish();
        done.insert(g.to_owned(), k);
        k
    }
    let mut done = HashMap::new();
    let mut on_path = HashSet::new();
    for g in reached {
        key_of(g, (routes, sources), own, &mut done, &mut on_path);
    }
    done
}

/// Per track with a Signal instance, the tracks its blocks key from; a
/// track missing has none, `None` is unknown (see [`key_sources`]).
type Sources = HashMap<String, Option<Vec<String>>>;

/// What each Signal track's blocks key from: every `key.from` of its
/// instances' `signal.chain/1` (a port, which is the track's path), as
/// track GUIDs. `None` for a track whose chain can't be read, or names a
/// port no track has: its dependencies are unknown.
fn key_sources(
    daw: &Standalone,
    live: &HashMap<String, Vec<Instance>>,
    all: &[daw_proto::Track],
) -> Sources {
    let paths = crate::mix_sends::paths(all);
    let by_path: HashMap<&str, &str> = paths
        .iter()
        .map(|(g, p)| (p.as_str(), g.as_str()))
        .collect();
    let from_of = |i: &Instance| -> Option<Vec<String>> {
        let json = daw
            .with_plugin_instance(&i.guid, |p| {
                p.signal()
                    .map(|s| crate::clap_host::plugin_call(|| s.chain()))
            })
            .flatten()?
            .ok()?;
        let chain: serde_json::Value = serde_json::from_str(&json).ok()?;
        chain["phases"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p["blocks"].as_array())
            .flatten()
            .filter_map(|b| b["key"]["from"].as_str())
            .map(|port| by_path.get(port).map(|g| (*g).to_owned()))
            .collect()
    };
    live.iter()
        .filter_map(|(track, fx)| {
            let signal: Vec<&Instance> = fx
                .iter()
                .filter(|i| crate::signal_kit::is_signal_bundle(&i.name))
                .collect();
            if signal.is_empty() {
                return None;
            }
            let from: Option<Vec<String>> = signal
                .into_iter()
                .map(from_of)
                .collect::<Option<Vec<Vec<String>>>>()
                .map(|v| v.concat());
            Some((track.clone(), from))
        })
        .collect()
}

/// A fingerprint of a plugin's state.
fn fingerprint(state: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    state.hash(&mut h);
    h.finish()
}

/// A plugin with a live instance: its guid, chain name and state.
#[derive(Clone)]
struct Instance {
    guid: String,
    name: String,
    state: Vec<u8>,
    /// A fingerprint of the values the song holds over its parameters
    /// (FX_SET on a hosted plugin), sent to it every block.
    held: u64,
}

/// The live plugins on `tracks`' chains, by track, their states read.
fn instances(
    daw: &Standalone,
    song: &str,
    tracks: &HashSet<String>,
) -> HashMap<String, Vec<Instance>> {
    let names: HashMap<String, String> = hosted(daw, song).into_iter().collect();
    let held_print = |params: &HashMap<u32, f64>| {
        let mut held: Vec<(u32, u64)> = params.iter().map(|(k, v)| (*k, v.to_bits())).collect();
        held.sort_unstable();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        held.hash(&mut h);
        h.finish()
    };
    let chains: Vec<(String, Vec<(String, u64)>)> = daw
        .read_project(song, |p| {
            p.fx_chains
                .iter()
                .filter_map(|(key, chain)| match key {
                    daw_standalone::sync::FxChainKey::Track(t) if tracks.contains(t) => Some((
                        t.clone(),
                        chain
                            .iter()
                            .map(|e| (e.fx.guid.clone(), held_print(&e.params)))
                            .collect(),
                    )),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    chains
        .into_iter()
        .map(|(track, fx)| {
            let live = fx
                .into_iter()
                .filter_map(|(g, held)| {
                    let name = names.get(&g)?.clone();
                    let state = daw.save_plugin_state(&g).unwrap_or_default();
                    Some(Instance {
                        guid: g,
                        name,
                        state,
                        held,
                    })
                })
                .collect();
            (track, live)
        })
        .collect()
}

/// What [`ready`] made ready.
struct Ready {
    plugins: Arc<Plugins>,
    /// Each copy's guid and the state and held values it carries.
    kept: Vec<(String, u64, u64)>,
    copied: u32,
    reused: u32,
    failures: Vec<String>,
}

/// A copy of each of `wanted` carrying its live instance's state: the warm
/// one when its state is unchanged (reset, so it starts as from silence),
/// else made again.
fn ready(
    daw: &Standalone,
    wanted: Vec<Instance>,
    warm: &mut HashMap<String, Warm>,
    (cuts, tag): (&HashMap<String, Cut>, &str),
) -> Result<Ready, String> {
    let restate = |name: &str, state: Vec<u8>| {
        if crate::signal_kit::is_signal_bundle(name) {
            crate::signal_kit::private_session(state, tag)
        } else {
            state
        }
    };
    let how = Copying {
        make: Some(&crate::mix_measure::make_fresh),
        restate: Some(&restate),
    };
    let plugins = Arc::new(Plugins::default());
    let mut out = Ready {
        plugins: plugins.clone(),
        kept: Vec::new(),
        copied: 0,
        reused: 0,
        failures: Vec::new(),
    };
    for i in wanted {
        let print = fingerprint(&i.state);
        if let Some(cut) = cuts.get(&i.guid) {
            let (mut copy, _) = copy_plugin(daw, &i.name, i.state, how)?;
            cut_copy(&mut *copy, cut)?;
            crate::clap_host::hand_copy(daw, &i.guid, &*copy);
            out.copied += 1;
            plugins.write().insert(i.guid, copy);
            continue;
        }
        let usable = warm.remove(&i.guid).and_then(|mut w| {
            let fits =
                w.state == print && (w.held == i.held || reload(&mut *w.plugin, &i, &restate));
            if fits {
                Some(w)
            } else {
                drop_later(vec![w.plugin]);
                None
            }
        });
        let copy = match usable {
            Some(mut w) => {
                if !w.plugin.reset() {
                    w.plugin.deactivate();
                }
                out.reused += 1;
                Ok(w.plugin)
            }
            None => copy_plugin(daw, &i.name, i.state, how).map(|(copy, _)| {
                out.copied += 1;
                copy
            }),
        };
        match copy {
            Ok(copy) => {
                // The song's placed hits as the live instance has them now
                // (a warm copy's may be an edit step old).
                crate::clap_host::hand_copy(daw, &i.guid, &*copy);
                plugins.write().insert(i.guid.clone(), copy);
                out.kept.push((i.guid, print, i.held));
            }
            Err(why) => out.failures.push(format!("{}: {why}", i.guid)),
        }
    }
    Ok(out)
}

/// Load `live`'s state (through `restate`) into a warm copy whose held
/// values have changed since it rendered: whether it took it (a plugin
/// with no state of its own, or one refusing it, is made again).
fn reload(
    copy: &mut dyn PluginInstance,
    live: &Instance,
    restate: &dyn Fn(&str, Vec<u8>) -> Vec<u8>,
) -> bool {
    if live.state.is_empty() {
        return false;
    }
    let state = restate(&live.name, live.state.clone());
    let took = copy.load_state(&state).is_ok();
    if took {
        copy.restart_if_requested();
    }
    took
}

/// Copies dropped off the action's thread: tearing instances down takes
/// time, and nothing waits for it.
fn drop_later(copies: Vec<Box<dyn PluginInstance>>) {
    if copies.is_empty() {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("listen-copies-drop".into())
        .spawn(move || drop(copies));
    if spawned.is_err() {
        tracing::warn!("listen: the copies are dropped here, a thread would not start");
    }
}

/// The render frames a listen of `from`..`to` runs over: from a preroll
/// before `from`; `from`; `to`.
fn frames_of(from: f64, to: f64) -> (u64, u64, u64) {
    let frame = |s: f64| (s * f64::from(RATE)).round().max(0.0) as u64;
    let (from, to) = (frame(from), frame(to));
    (from.saturating_sub(frame(PREROLL)), from, to)
}

/// Each track's own fingerprint: as a render reads it (`prints`), and its
/// plugins' states.
fn own_prints(
    prints: Vec<(String, u64)>,
    plugins: &HashMap<String, Vec<Instance>>,
    cuts: &HashMap<String, Cut>,
) -> HashMap<String, u64> {
    prints
        .into_iter()
        .map(|(g, p)| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            p.hash(&mut h);
            for i in plugins.get(&g).into_iter().flatten() {
                fingerprint(&i.state).hash(&mut h);
                i.held.hash(&mut h);
                cuts.get(&i.guid).hash(&mut h);
            }
            (g, h.finish())
        })
        .collect()
}

/// What a render will do: render `rendered` (the reached tracks not
/// played back), play `fed` back.
struct Plan {
    answered: Answered,
    rendered: HashSet<String>,
    fed: HashSet<String>,
}

/// What a listen of `points` reaching `everything` renders, given which
/// points have a recording (`recorded`), which tracks' output does
/// (`replayable`), and what each track's plugins key from (`sources`:
/// the tracks whose Signal instances publish the hits its blocks follow;
/// `None` when that is unknown, and everything renders).
///
/// A rendered track's key sources render with it, played back never: what
/// is needed of them is the hits they publish as they run, not their
/// audio. What else of the session runs makes no difference to it.
fn plan(
    routes: &Routes,
    (points, everything): (&[ProbeAt], &HashSet<String>),
    recorded: impl Fn(&ProbeAt) -> bool,
    replayable: impl Fn(&str) -> bool,
    sources: impl Fn(&str) -> Option<Vec<String>>,
) -> Plan {
    if points.iter().all(&recorded) {
        return Plan {
            answered: Answered::Replayed,
            rendered: HashSet::new(),
            fed: HashSet::new(),
        };
    }
    // A point inside a track needs the track rendered; a point after its
    // fader is its output, which may be played back.
    let inside: HashSet<&str> = points
        .iter()
        .filter(|p| p.at != Probe::PostFader)
        .map(|p| p.track.as_str())
        .collect();
    let everything_rendered = || Plan {
        answered: Answered::Rendered,
        rendered: everything.clone(),
        fed: HashSet::new(),
    };
    let roots: Vec<String> = points.iter().map(|p| p.track.clone()).collect();
    let stop = |g: &str| !inside.contains(g) && replayable(g);
    let (reached, mut fed) = reach(routes, roots, stop);
    let mut rendered: HashSet<String> = reached.difference(&fed).cloned().collect();
    let mut todo: Vec<String> = rendered.iter().cloned().collect();
    while let Some(g) = todo.pop() {
        let Some(from) = sources(&g) else {
            return everything_rendered();
        };
        for s in from {
            if rendered.contains(&s) {
                continue;
            }
            if !everything.contains(&s) {
                return everything_rendered();
            }
            fed.remove(&s);
            rendered.insert(s.clone());
            todo.push(s.clone());
            // What feeds a source: played back where it can be.
            let (more, stopped) = reach(routes, routes.feeders(&s).cloned(), stop);
            for m in more.difference(&stopped) {
                if rendered.insert(m.clone()) {
                    fed.remove(m);
                    todo.push(m.clone());
                }
            }
            fed.extend(stopped.into_iter().filter(|m| !rendered.contains(m)));
        }
    }
    if fed.is_empty() {
        return everything_rendered();
    }
    Plan {
        answered: Answered::Partly,
        rendered,
        fed,
    }
}

/// Render `ask` of song `song` (`project` its context): what feeds its
/// points, played back where nothing it hears has changed.
///
/// # Errors
/// The span is empty or too long.
pub fn render(
    daw: &Standalone,
    project: &ProjectContext,
    song: &str,
    ask: &Ask<'_>,
) -> Result<Heard, String> {
    let began = web_time::Instant::now();
    if !(ask.to > ask.from) {
        return Err(format!(
            "`to` ({:.3}s) is not after `from` ({:.3}s)",
            ask.to, ask.from
        ));
    }
    if ask.to - ask.from > MOST {
        return Err(format!(
            "{:.0} s is longer than the {MOST:.0} s listened to at once",
            ask.to - ask.from
        ));
    }
    let all = daw::service::Tracks::all(daw, project.clone());
    let routes = Routes::new(
        all.iter()
            .map(|t| (t.guid.as_str(), t.parent_guid.as_deref(), t.parent_send)),
        sends_into(daw, project, &all),
    );
    let roots: Vec<String> = ask.points.iter().map(|p| p.track.clone()).collect();
    let mut everything = reach_keyed(&routes, &roots, ask.keyed);
    let mut live = instances(daw, song, &everything);
    // What the reached Signal tracks key from runs too (it publishes the
    // hits they follow), with what feeds it.
    let mut sources = key_sources(daw, &live, &all);
    loop {
        let extra: Vec<String> = sources
            .values()
            .flatten()
            .flatten()
            .filter(|g| !everything.contains(*g))
            .cloned()
            .collect();
        if extra.is_empty() {
            break;
        }
        let (more, _) = reach(&routes, extra, |_| false);
        let new: HashSet<String> = more.difference(&everything).cloned().collect();
        live.extend(instances(daw, song, &new));
        everything.extend(new);
        sources = key_sources(daw, &live, &all);
    }
    // The song as this render reads it: its solos and bypasses.
    let prints = ProjectRenderer::offline(
        daw,
        song,
        RATE,
        Offline {
            solo: ask.solo.clone(),
            bypass_fx: ask.bypass_fx.clone(),
            ..Offline::default()
        },
    )
    .track_prints();
    let keys = keys(
        &routes,
        &everything,
        (&own_prints(prints, &live, ask.cuts), &sources),
    );
    let (start, from, to) = frames_of(ask.from, ask.to);
    let mut kept = kept()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let key = |g: &str| keys.get(g).copied();
    let sources_of = |g: &str| match sources.get(g) {
        Some(known) => known.clone(),
        None => Some(Vec::new()),
    };
    let planned = if ask.fresh {
        Plan {
            answered: Answered::Rendered,
            rendered: everything.clone(),
            fed: HashSet::new(),
        }
    } else {
        plan(
            &routes,
            (ask.points, &everything),
            |p| {
                kept.holding(
                    &(p.track.clone(), p.at),
                    (key(&p.track), ask.exact),
                    (start, to),
                )
                .is_some()
            },
            |g| {
                !routes.taps_before_its_fader(g)
                    && kept
                        .holding(
                            &(g.to_owned(), Probe::PostFader),
                            (key(g), ask.exact),
                            (start, to),
                        )
                        .is_some()
            },
            sources_of,
        )
    };
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    if planned.answered == Answered::Replayed {
        let (signals, latencies): (Vec<Vec<f32>>, Vec<u32>) = ask
            .points
            .iter()
            .filter_map(|p| {
                kept.holding(
                    &(p.track.clone(), p.at),
                    (key(&p.track), ask.exact),
                    (start, to),
                )
            })
            .map(|r| {
                (
                    line_up(&r.samples, r.start, (from, to), r.latency),
                    r.latency,
                )
            })
            .unzip();
        let mut replayed: Vec<String> = roots.clone();
        replayed.sort();
        replayed.dedup();
        let plan_ms = began.elapsed().as_secs_f64() * 1e3;
        return Ok(Heard {
            signals,
            latencies,
            answered: Answered::Replayed,
            rendered: Vec::new(),
            replayed,
            song_tracks: count(all.len()),
            copied: 0,
            reused: 0,
            failures: Vec::new(),
            chunks: 0,
            plan_ms,
            copy_ms: 0.0,
            prime_ms: 0.0,
            blocks_ms: 0.0,
            ms: plan_ms,
        });
    }
    let fed_map: HashMap<String, Fed> = planned
        .fed
        .iter()
        .filter_map(|g| {
            let r = kept.holding(
                &(g.clone(), Probe::PostFader),
                (key(g), ask.exact),
                (start, to),
            )?;
            Some((
                g.clone(),
                Fed {
                    start: r.start,
                    samples: r.samples.clone(),
                    latency: r.latency,
                },
            ))
        })
        .collect();
    let wanted: Vec<Instance> = planned
        .rendered
        .iter()
        .filter_map(|g| live.remove(g))
        .flatten()
        .collect();
    let plan_ms = began.elapsed().as_secs_f64() * 1e3;
    // The points asked for, then every rendered track's output to keep.
    let mut probes = ask.points.to_vec();
    probes.extend(planned.rendered.iter().map(|g| ProbeAt {
        track: g.clone(),
        at: Probe::PostFader,
    }));
    let mut only = planned.rendered.clone();
    only.extend(planned.fed.iter().cloned());
    let spans = if ask.exact {
        vec![(start, start, to)]
    } else {
        chunks(start, from, to)
    };
    if kept.warm.len() < spans.len() {
        kept.warm.resize_with(spans.len(), HashMap::new);
    }
    let shared = Shared {
        daw,
        song,
        ask,
        probes: &probes,
        only: &only,
        fed: &fed_map,
    };
    let chunk_count = spans.len();
    let outs: Vec<Result<ChunkOut, String>> = std::thread::scope(|s| {
        let running: Vec<_> = spans
            .iter()
            .zip(kept.warm.iter_mut())
            .enumerate()
            .map(|(k, (span, warm))| {
                let (wanted, shared) = (wanted.clone(), &shared);
                let last = k + 1 == chunk_count;
                s.spawn(move || run_chunk(shared, ((k, last), *span), wanted, warm))
            })
            .collect();
        running
            .into_iter()
            .map(|r| {
                r.join()
                    .unwrap_or_else(|_| Err("a chunk's render panicked".to_owned()))
            })
            .collect()
    });
    let outs: Vec<ChunkOut> = outs.into_iter().collect::<Result<_, _>>()?;
    // Each probe as rendered, from the preroll's start until it reached
    // `to`, late by its latency.
    let raw: Vec<Vec<f32>> = (0..probes.len())
        .map(|i| outs.iter().flat_map(|o| o.raw[i].iter().copied()).collect())
        .collect();
    let latencies: Vec<u32> = outs
        .first()
        .map(|o| o.latencies.clone())
        .unwrap_or_default();
    let n = ask.points.len();
    let signals: Vec<Vec<f32>> = raw[..n]
        .iter()
        .zip(&latencies)
        .map(|(r, late)| line_up(r, start, (from, to), *late))
        .collect();
    let covered = |r: &[f32]| start + (r.len() / 2) as u64;
    for ((p, samples), late) in probes.iter().zip(raw).zip(&latencies) {
        if !planned.rendered.contains(&p.track) {
            continue;
        }
        let Some(key) = key(&p.track) else { continue };
        kept.keep(
            (p.track.clone(), p.at),
            Recorded {
                key,
                exact: ask.exact || spans.len() == 1,
                start,
                end: covered(&samples),
                samples: samples.into(),
                latency: *late,
                stamp: 0,
            },
        );
    }
    let most = |f: fn(&ChunkOut) -> f64| outs.iter().map(f).fold(0.0, f64::max);
    Ok(Heard {
        signals,
        latencies: outs
            .first()
            .map(|o| o.latencies[..n].to_vec())
            .unwrap_or_default(),
        answered: planned.answered,
        rendered: planned.rendered.into_iter().collect(),
        replayed: planned.fed.into_iter().collect(),
        song_tracks: count(all.len()),
        copied: outs.iter().map(|o| o.copied).sum(),
        reused: outs.iter().map(|o| o.reused).sum(),
        failures: outs
            .iter()
            .flat_map(|o| o.failures.iter().cloned())
            .collect(),
        chunks: count(outs.len()),
        plan_ms,
        copy_ms: most(|o| o.copy_ms),
        prime_ms: most(|o| o.prime_ms),
        blocks_ms: most(|o| o.blocks_ms),
        ms: began.elapsed().as_secs_f64() * 1e3,
    })
}

/// The chunks a listen of render frames `start..to` (`from` the span's
/// first) is rendered in: each one's first rendered frame (a preroll
/// before its own first), its first kept frame and its end. The first
/// keeps its preroll too, so what is kept starts at `start`.
fn chunks(start: u64, from: u64, to: u64) -> Vec<(u64, u64, u64)> {
    let least = (CHUNK_LEAST * f64::from(RATE)) as u64;
    let n = ((to - from) / least.max(1)).clamp(1, CHUNKS);
    let preroll = (PREROLL * f64::from(RATE)) as u64;
    let edge = |k: u64| from + (to - from) * k / n;
    (0..n)
        .map(|k| {
            if k == 0 {
                (start, start, edge(1))
            } else {
                (edge(k).saturating_sub(preroll), edge(k), edge(k + 1))
            }
        })
        .collect()
}

/// What every chunk of one render reads.
struct Shared<'a> {
    daw: &'a Standalone,
    song: &'a str,
    ask: &'a Ask<'a>,
    probes: &'a [ProbeAt],
    only: &'a HashSet<String>,
    fed: &'a HashMap<String, Fed>,
}

/// What one chunk heard.
struct ChunkOut {
    /// Each probe as it came out, late by its latency, over the chunk's
    /// kept render frames (the first chunk's from its preroll's start, the
    /// last's on until every probe has reached the span's end).
    raw: Vec<Vec<f32>>,
    latencies: Vec<u32>,
    copied: u32,
    reused: u32,
    failures: Vec<String>,
    copy_ms: f64,
    prime_ms: f64,
    blocks_ms: f64,
}

/// Render chunk `k` (its frames `(first rendered, first kept, end)`) on
/// its own copies (`warm` its kept ones), in a Signal session of its own.
fn run_chunk(
    c: &Shared<'_>,
    ((k, last_chunk), (first, keep, end)): ((usize, bool), (u64, u64, u64)),
    wanted: Vec<Instance>,
    warm: &mut HashMap<String, Warm>,
) -> Result<ChunkOut, String> {
    let copying = web_time::Instant::now();
    let tag = format!("{SESSION_TAG}-{k}");
    let ready = ready(c.daw, wanted, warm, (c.ask.cuts, &tag))?;
    let copy_ms = copying.elapsed().as_secs_f64() * 1e3;
    let renderer = ProjectRenderer::offline(
        c.daw,
        c.song,
        RATE,
        Offline {
            plugins: ready.plugins.clone(),
            bypass_fx: c.ask.bypass_fx.clone(),
            solo: c.ask.solo.clone(),
            tap: None,
            only: Some(c.only.clone()),
            probes: c.probes.to_vec(),
            fed: c.fed.clone(),
        },
    );
    let primed = web_time::Instant::now();
    renderer.prime(u32::try_from(BLOCK).unwrap_or(1_024));
    let latencies = renderer.probe_latencies();
    let prime_ms = primed.elapsed().as_secs_f64() * 1e3;
    let blocks = web_time::Instant::now();
    let last = end + u64::from(latencies.iter().copied().max().unwrap_or(0));
    let raw = render_blocks(&renderer, (first, last), c.probes.len());
    let blocks_ms = blocks.elapsed().as_secs_f64() * 1e3;
    drop(renderer);
    // As rendered, not lined up: a recording played back later enters the
    // delay compensation exactly as this track did, so what it feeds is as
    // late (its plugins told the same song time) as in this render.
    let (from, to) = (
        if k == 0 { first } else { keep },
        if last_chunk { last } else { end },
    );
    let raw = raw
        .iter()
        .map(|r| {
            r.get(((from - first) * 2) as usize..((to - first) * 2) as usize)
                .map(<[f32]>::to_vec)
                .unwrap_or_default()
        })
        .collect();
    for (guid, state, held) in ready.kept {
        if let Some(plugin) = ready.plugins.write().remove(&guid) {
            warm.insert(
                guid,
                Warm {
                    state,
                    held,
                    plugin,
                },
            );
        }
    }
    Ok(ChunkOut {
        raw,
        latencies,
        copied: ready.copied,
        reused: ready.reused,
        failures: ready.failures,
        copy_ms,
        prime_ms,
        blocks_ms,
    })
}

/// The renderer's blocks over render frames `start..end`: every probe's
/// signal as it came, interleaved stereo.
fn render_blocks(
    renderer: &ProjectRenderer,
    (start, end): (u64, u64),
    probes: usize,
) -> Vec<Vec<f32>> {
    let frames = (end - start) as usize;
    let mut out: Vec<Vec<f32>> = (0..probes)
        .map(|_| Vec::with_capacity(frames * 2))
        .collect();
    let mut at = start;
    while at < end {
        let n = BLOCK.min((end - at) as usize);
        renderer.render_block(at, n);
        renderer.with_probes(|got| {
            for (dst, src) in out.iter_mut().zip(got) {
                dst.extend_from_slice(&src[..(n * 2).min(src.len())]);
            }
        });
        at += n as u64;
    }
    out
}

/// A probe's signal (from render frame `start`, `late` samples late) as
/// the timeline's frames `from..to`.
fn line_up(raw: &[f32], start: u64, (from, to): (u64, u64), late: u32) -> Vec<f32> {
    let len = ((to - from) * 2) as usize;
    let skip = ((from - start + u64::from(late)) * 2) as usize;
    let mut out: Vec<f32> = raw.iter().skip(skip).take(len).copied().collect();
    out.resize(len, 0.0);
    out
}

/// Let everything kept go (the next listen renders and copies afresh):
/// how many copies and recordings there were.
pub fn forget() -> (u32, u32) {
    let mut kept = kept()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let counts = (
        u32::try_from(kept.warm.iter().map(HashMap::len).sum::<usize>()).unwrap_or(u32::MAX),
        u32::try_from(kept.recordings.len()).unwrap_or(u32::MAX),
    );
    drop_later(
        kept.warm
            .iter_mut()
            .flat_map(|w| w.drain().map(|(_, w)| w.plugin))
            .collect(),
    );
    kept.recordings.clear();
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kick ⊃ Sum ⊃ {In, Out, Trig (not to its folder)}, Sub fed by a send
    /// from Out; Snare apart.
    fn kit() -> Routes {
        let tracks = [
            ("kick", None, true),
            ("sum", Some("kick"), true),
            ("in", Some("sum"), true),
            ("out", Some("sum"), true),
            ("trig", Some("sum"), false),
            ("sub", Some("kick"), true),
            ("snare", None, true),
        ];
        let sends = [(
            "sub".to_owned(),
            vec![Into {
                from: "out".into(),
                mode: SendMode::PostFader,
            }],
        )]
        .into_iter()
        .collect();
        Routes::new(tracks, sends)
    }

    fn set(v: &[&str]) -> HashSet<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    fn at(track: &str, at: Probe) -> ProbeAt {
        ProbeAt {
            track: track.into(),
            at,
        }
    }

    #[test]
    fn a_listen_reaches_what_feeds_it_its_triggers_too() {
        let routes = kit();
        let (all, stopped) = reach(&routes, ["kick".to_owned()], |_| false);
        assert_eq!(all, set(&["kick", "sum", "in", "out", "sub"]));
        assert!(stopped.is_empty());
        let (all, stopped) = reach(&routes, ["kick".to_owned()], |g| g == "sum");
        assert_eq!(stopped, set(&["sum"]));
        assert_eq!(all, set(&["kick", "sum", "sub", "out"]));
        let keyed = [Keyed {
            from: "trig".into(),
            by: vec!["in".into(), "out".into()],
        }];
        let all = reach_keyed(&routes, &["sum".to_owned()], &keyed);
        assert_eq!(all, set(&["sum", "in", "out", "trig"]));
        let all = reach_keyed(&routes, &["snare".to_owned()], &keyed);
        assert_eq!(all, set(&["snare"]));
    }

    #[test]
    fn nothing_changed_is_replayed_and_a_change_renders_what_it_reaches_and_its_keys() {
        let routes = kit();
        let everything = reach_keyed(&routes, &["kick".to_owned()], &[]);
        let everything: HashSet<String> = everything.into_iter().chain(["trig".into()]).collect();
        let points = [at("kick", Probe::PostFader)];
        let no_keys = |_: &str| Some(Vec::new());
        let p = plan(&routes, (&points, &everything), |_| true, |_| true, no_keys);
        assert_eq!(p.answered, Answered::Replayed);
        // Kick changed, its feeders not: Kick alone renders.
        let p = plan(
            &routes,
            (&points, &everything),
            |_| false,
            |g| g != "kick",
            no_keys,
        );
        assert_eq!(p.answered, Answered::Partly);
        assert_eq!(p.rendered, set(&["kick"]));
        assert_eq!(p.fed, set(&["sum", "sub"]));
        // Sub changed, and its gate keys from Trig: Trig runs with it (its
        // hits), played back never; Out, which sends into Sub, plays back.
        let keys = |g: &str| {
            Some(if g == "sub" {
                vec!["trig".to_owned()]
            } else {
                Vec::new()
            })
        };
        let p = plan(
            &routes,
            (&points, &everything),
            |_| false,
            |g| g != "kick" && g != "sub",
            keys,
        );
        assert_eq!(p.answered, Answered::Partly);
        assert_eq!(p.rendered, set(&["kick", "sub", "trig"]));
        assert_eq!(p.fed, set(&["sum", "out"]));
        // What Sub keys from unknown: everything renders.
        let unknown = |g: &str| if g == "sub" { None } else { Some(Vec::new()) };
        let p = plan(
            &routes,
            (&points, &everything),
            |_| false,
            |g| g != "kick" && g != "sub",
            unknown,
        );
        assert_eq!(p.answered, Answered::Rendered);
        assert_eq!(p.rendered, everything);
        // A point inside Kick: Kick renders even with a recording of it.
        let inside = [at("kick", Probe::FxOut(0))];
        let p = plan(
            &routes,
            (&inside, &everything),
            |_| false,
            |_| true,
            no_keys,
        );
        assert_eq!(p.rendered, set(&["kick"]));
    }

    #[test]
    fn a_cut_bypasses_the_block_on_and_what_follows_it() {
        let chain = serde_json::json!({"phases": [
            {"id": "rescue", "blocks": [{"id": "rescue.gate"}, {"id": "rescue.eq"}]},
            {"id": "tone", "blocks": [{"id": "tone.comp"}]},
        ]});
        let words = |side| {
            cut_actions(
                &chain,
                &Cut {
                    block: "rescue.gate".into(),
                    side,
                },
            )
            .unwrap()
            .into_iter()
            .map(|(_, a)| a)
            .collect::<Vec<_>>()
        };
        assert_eq!(
            words(Side::Pre),
            vec![
                r#"{"block":"rescue.gate","bypass":true}"#,
                r#"{"block":"rescue.eq","bypass":true}"#,
                r#"{"makeup_db":0.0,"phase":"rescue"}"#,
                r#"{"bypass":true,"phase":"tone"}"#,
            ]
        );
        assert_eq!(words(Side::Post).len(), 3);
        assert!(
            cut_actions(
                &chain,
                &Cut {
                    block: "x".into(),
                    side: Side::Pre
                }
            )
            .is_err()
        );
    }

    #[test]
    fn a_change_upstream_changes_every_key_downstream_and_no_other() {
        let routes = kit();
        let (all, _) = reach(&routes, ["kick".to_owned()], |_| false);
        let own: HashMap<String, u64> = all.iter().map(|g| (g.clone(), 1)).collect();
        let before = keys(&routes, &all, &own);
        let mut changed = own.clone();
        changed.insert("in".into(), 2);
        let after = keys(&routes, &all, &changed);
        for g in ["in", "sum", "kick"] {
            assert_ne!(before[g], after[g], "{g}");
        }
        for g in ["out", "sub"] {
            assert_eq!(before[g], after[g], "{g}");
        }
    }

    #[test]
    fn a_span_is_cut_in_chunks_each_with_its_own_preroll() {
        let s = u64::from(RATE);
        // 12 s from 10 s, a 1 s preroll: three 4 s chunks.
        let c = chunks(9 * s, 10 * s, 22 * s);
        assert_eq!(
            c,
            vec![
                (9 * s, 9 * s, 14 * s),
                (13 * s, 14 * s, 18 * s),
                (17 * s, 18 * s, 22 * s)
            ]
        );
        // Under two chunks' worth: one.
        assert_eq!(chunks(0, s, 6 * s).len(), 1);
    }

    #[test]
    fn a_probe_is_lined_up_by_its_lateness() {
        // Frames 0..10 rendered from frame 0; the timeline's 2..5 asked,
        // the probe 3 late: its frames 5..8.
        let raw: Vec<f32> = (0..10).flat_map(|f| [f as f32, f as f32]).collect();
        assert_eq!(
            line_up(&raw, 0, (2, 5), 3),
            vec![5.0, 5.0, 6.0, 6.0, 7.0, 7.0]
        );
    }
}
