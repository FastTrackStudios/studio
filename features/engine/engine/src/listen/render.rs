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
//!   is played back in place (`Offline::fed`) — but only where no Signal
//!   instance must run: Signal's instances in one session hear each other
//!   (a folder's chain follows its children's ports), so once one of them
//!   renders, every Signal track the listen reaches renders with it.
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
const KEPT_RECORDINGS: usize = 160;
/// The Signal session the copies share: one of their own, never the live
/// kit's, and the same for every listen so a reused copy and a fresh one
/// meet in it.
const SESSION_TAG: &str = "listen";

/// A track whose plugins others key from (`from`, a kit piece's
/// trigger), and the tracks keyed (`by`, its piece's mics).
#[derive(Clone, Debug)]
pub struct Keyed {
    pub from: String,
    pub by: Vec<String>,
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
    pub plan_ms: f64,
    pub copy_ms: f64,
    pub prime_ms: f64,
    pub blocks_ms: f64,
    pub ms: f64,
}

/// A copy of a live plugin, and the fingerprint of the state it was made
/// from.
struct Warm {
    fingerprint: u64,
    plugin: Box<dyn PluginInstance>,
}

/// One point of a track as a render heard it.
struct Recorded {
    key: u64,
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
    warm: HashMap<String, Warm>,
    recordings: HashMap<Point, Recorded>,
    stamp: u64,
}

impl Kept {
    /// Keep a recording, the oldest let go past [`KEPT_RECORDINGS`].
    fn keep(&mut self, at: Point, mut r: Recorded) {
        self.stamp += 1;
        r.stamp = self.stamp;
        self.recordings.insert(at, r);
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
    fn holding(&self, at: &Point, key: Option<u64>, frames: (u64, u64)) -> Option<&Recorded> {
        self.recordings
            .get(at)
            .filter(|r| key.is_some_and(|k| r.holds(k, frames)))
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
    own: &HashMap<String, u64>,
) -> HashMap<String, u64> {
    fn key_of(
        g: &str,
        routes: &Routes,
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
                key_of(f, routes, own, done, on_path).hash(&mut h);
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
        key_of(g, routes, own, &mut done, &mut on_path);
    }
    done
}

/// A fingerprint of a plugin's state.
fn fingerprint(state: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    state.hash(&mut h);
    h.finish()
}

/// A plugin with a live instance: its guid, chain name and state.
struct Instance {
    guid: String,
    name: String,
    state: Vec<u8>,
}

/// The live plugins on `tracks`' chains, by track, their states read.
fn instances(
    daw: &Standalone,
    song: &str,
    tracks: &HashSet<String>,
) -> HashMap<String, Vec<Instance>> {
    let names: HashMap<String, String> = hosted(daw, song).into_iter().collect();
    let chains: Vec<(String, Vec<String>)> = daw
        .read_project(song, |p| {
            p.fx_chains
                .iter()
                .filter_map(|(key, chain)| match key {
                    daw_standalone::sync::FxChainKey::Track(t) if tracks.contains(t) => {
                        Some((t.clone(), chain.iter().map(|e| e.fx.guid.clone()).collect()))
                    }
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
                .filter_map(|g| {
                    let name = names.get(&g)?.clone();
                    let state = daw.save_plugin_state(&g).unwrap_or_default();
                    Some(Instance {
                        guid: g,
                        name,
                        state,
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
    /// Each copy's guid and the fingerprint it carries.
    kept: Vec<(String, u64)>,
    copied: u32,
    reused: u32,
    failures: Vec<String>,
}

/// A copy of each of `wanted` carrying its live instance's state: the warm
/// one when its state is unchanged (reset, so it starts as from silence),
/// else made again.
fn ready(daw: &Standalone, wanted: Vec<Instance>, warm: &mut HashMap<String, Warm>) -> Ready {
    let restate = |name: &str, state: Vec<u8>| {
        if crate::signal_kit::is_signal_bundle(name) {
            crate::signal_kit::private_session(state, SESSION_TAG)
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
        let copy = match warm.remove(&i.guid) {
            Some(mut w) if w.fingerprint == print => {
                if !w.plugin.reset() {
                    w.plugin.deactivate();
                }
                out.reused += 1;
                Ok(w.plugin)
            }
            stale => {
                drop_later(stale.map(|w| w.plugin).into_iter().collect());
                copy_plugin(daw, &i.name, i.state, how).map(|(copy, _)| {
                    out.copied += 1;
                    copy
                })
            }
        };
        match copy {
            Ok(copy) => {
                plugins.write().insert(i.guid.clone(), copy);
                out.kept.push((i.guid, print));
            }
            Err(why) => out.failures.push(format!("{}: {why}", i.guid)),
        }
    }
    out
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
) -> HashMap<String, u64> {
    prints
        .into_iter()
        .map(|(g, p)| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            p.hash(&mut h);
            for i in plugins.get(&g).into_iter().flatten() {
                fingerprint(&i.state).hash(&mut h);
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
/// (`replayable`), and which tracks carry a Signal instance (`coupled`).
fn plan(
    routes: &Routes,
    (points, everything): (&[ProbeAt], &HashSet<String>),
    recorded: impl Fn(&ProbeAt) -> bool,
    replayable: impl Fn(&str) -> bool,
    coupled: impl Fn(&str) -> bool,
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
    let roots: Vec<String> = points.iter().map(|p| p.track.clone()).collect();
    let (reached, fed) = reach(routes, roots, |g| !inside.contains(g) && replayable(g));
    let rendered: HashSet<String> = reached.difference(&fed).cloned().collect();
    if fed.is_empty() || rendered.iter().any(|g| coupled(g)) {
        return Plan {
            answered: Answered::Rendered,
            rendered: everything.clone(),
            fed: HashSet::new(),
        };
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
    let everything = reach_keyed(&routes, &roots, ask.keyed);
    let mut live = instances(daw, song, &everything);
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
    let keys = keys(&routes, &everything, &own_prints(prints, &live));
    let (start, from, to) = frames_of(ask.from, ask.to);
    let mut kept = kept()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let key = |g: &str| keys.get(g).copied();
    let has_signal = |g: &str| {
        live.get(g).is_some_and(|i| {
            i.iter()
                .any(|i| crate::signal_kit::is_signal_bundle(&i.name))
        })
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
                kept.holding(&(p.track.clone(), p.at), key(&p.track), (start, to))
                    .is_some()
            },
            |g| {
                !routes.taps_before_its_fader(g)
                    && kept
                        .holding(&(g.to_owned(), Probe::PostFader), key(g), (start, to))
                        .is_some()
            },
            has_signal,
        )
    };
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    if planned.answered == Answered::Replayed {
        let (signals, latencies): (Vec<Vec<f32>>, Vec<u32>) = ask
            .points
            .iter()
            .filter_map(|p| kept.holding(&(p.track.clone(), p.at), key(&p.track), (start, to)))
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
            let r = kept.recordings.get(&(g.clone(), Probe::PostFader))?;
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
    let copying = web_time::Instant::now();
    let ready = ready(daw, wanted, &mut kept.warm);
    let copy_ms = copying.elapsed().as_secs_f64() * 1e3;
    // The points asked for, then every rendered track's output to keep.
    let mut probes = ask.points.to_vec();
    probes.extend(planned.rendered.iter().map(|g| ProbeAt {
        track: g.clone(),
        at: Probe::PostFader,
    }));
    let mut only = planned.rendered.clone();
    only.extend(planned.fed.iter().cloned());
    let renderer = ProjectRenderer::offline(
        daw,
        song,
        RATE,
        Offline {
            plugins: ready.plugins.clone(),
            bypass_fx: ask.bypass_fx.clone(),
            solo: ask.solo.clone(),
            tap: None,
            only: Some(only),
            probes: probes.clone(),
            fed: fed_map,
        },
    );
    let primed = web_time::Instant::now();
    renderer.prime(u32::try_from(BLOCK).unwrap_or(1_024));
    let latencies = renderer.probe_latencies();
    let prime_ms = primed.elapsed().as_secs_f64() * 1e3;
    let blocks = web_time::Instant::now();
    let end = to + u64::from(latencies.iter().copied().max().unwrap_or(0));
    let raw = render_blocks(&renderer, (start, end), probes.len());
    let blocks_ms = blocks.elapsed().as_secs_f64() * 1e3;
    drop(renderer);
    let n = ask.points.len();
    let signals: Vec<Vec<f32>> = raw[..n]
        .iter()
        .zip(&latencies)
        .map(|(r, late)| line_up(r, start, (from, to), *late))
        .collect();
    for ((p, samples), late) in probes.iter().zip(raw).zip(&latencies) {
        if !planned.rendered.contains(&p.track) {
            continue;
        }
        let Some(key) = key(&p.track) else { continue };
        kept.keep(
            (p.track.clone(), p.at),
            Recorded {
                key,
                start,
                end,
                samples: samples.into(),
                latency: *late,
                stamp: 0,
            },
        );
    }
    for (guid, print) in ready.kept {
        if let Some(plugin) = ready.plugins.write().remove(&guid) {
            kept.warm.insert(
                guid,
                Warm {
                    fingerprint: print,
                    plugin,
                },
            );
        }
    }
    Ok(Heard {
        signals,
        latencies: latencies[..n].to_vec(),
        answered: planned.answered,
        rendered: planned.rendered.into_iter().collect(),
        replayed: planned.fed.into_iter().collect(),
        song_tracks: count(all.len()),
        copied: ready.copied,
        reused: ready.reused,
        failures: ready.failures,
        plan_ms,
        copy_ms,
        prime_ms,
        blocks_ms,
        ms: began.elapsed().as_secs_f64() * 1e3,
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
        u32::try_from(kept.warm.len()).unwrap_or(u32::MAX),
        u32::try_from(kept.recordings.len()).unwrap_or(u32::MAX),
    );
    drop_later(kept.warm.drain().map(|(_, w)| w.plugin).collect());
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
    fn nothing_changed_is_replayed_and_a_signal_track_rendering_renders_them_all() {
        let routes = kit();
        let everything = reach_keyed(&routes, &["kick".to_owned()], &[]);
        let points = [at("kick", Probe::PostFader)];
        let p = plan(
            &routes,
            (&points, &everything),
            |_| true,
            |_| true,
            |_| true,
        );
        assert_eq!(p.answered, Answered::Replayed);
        // Kick changed, its feeders not: Kick alone renders, unless it
        // carries Signal — then everything it reaches does.
        let p = plan(
            &routes,
            (&points, &everything),
            |_| false,
            |g| g != "kick",
            |_| false,
        );
        assert_eq!(p.answered, Answered::Partly);
        assert_eq!(p.rendered, set(&["kick"]));
        assert_eq!(p.fed, set(&["sum", "sub"]));
        let p = plan(
            &routes,
            (&points, &everything),
            |_| false,
            |g| g != "kick",
            |g| g == "kick",
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
            |_| false,
        );
        assert_eq!(p.rendered, set(&["kick"]));
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
