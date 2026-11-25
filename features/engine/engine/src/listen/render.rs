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
//!   A cold render's copies (MIX_MEASURE's) are never kept.
//!
//! The recordings are bounded by bytes (`STUDIO_LISTEN_CACHE_MB`, 32 GiB
//! unless set, or [`set_limit`]): past it, the one heard longest ago goes.
//! A track's key also carries the render's solos (a solo anywhere silences
//! the rest) and, for the master, its fader and the keys of what sums into
//! it.
//!
//! One render can hear the song several ways at once ([`Ask::more`]): a
//! block's input and output (two cuts of one Signal instance), the mix
//! with and without Signal. Each hearing is planned against the
//! recordings on its own and rendered side by side with the others, on
//! copies (and in Signal sessions) of its own.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use daw::service::ProjectContext;
use daw_proto::routing::SendMode;
use daw_standalone::audio_engine::render::ProjectRenderer;
use daw_standalone::audio_engine::render::offline::{
    Copying, Fed, Offline, Probe, ProbeAt, Record, copy_plugin, hosted,
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
    /// point at `block:<id>:pre|post`): copies of their own, cut, kept warm
    /// apart from the uncut ones.
    pub cuts: &'a HashMap<String, Cut>,
    /// Render in one piece, not in chunks side by side: slower, and what a
    /// whole render (MIX_MEASURE) would say to the sample. Chunks, each
    /// from a preroll of its own, measure within about 0.05 LU of it, and
    /// within a few tenths of a dB on peaks and crest.
    pub exact: bool,
    /// Signal instances (by FX guid) whose blocks' meters to summarise over
    /// `from`..`to` (`SIGNAL_METERS` on each chunk's copy): a gain
    /// reduction per block, read without playback.
    pub meters: &'a [String],
    /// Also hear the master (what every top-level track sums to, through
    /// its fader): [`Heard::master`].
    pub master: bool,
    /// Every plugin rendered on a copy made for this render, never a warm
    /// one reset (a reset is not trusted to clear every tail), and only
    /// recordings made that way played back: what a whole render would
    /// say, to the sample (with `exact`).
    pub cold: bool,
    /// More hearings of the same span in the same render, each its own
    /// points, cuts and bypasses, side by side with this one on copies of
    /// their own ([`Heard::more`], in order).
    pub more: &'a [Pass<'a>],
}

impl Ask<'_> {
    /// An ask of `points` over `from`..`to`, everything else as a plain
    /// listen has it (no solo, bypass, cut, meters or master; warm, in
    /// chunks).
    #[must_use]
    pub fn plain<'a>(
        (from, to): (f64, f64),
        points: &'a [ProbeAt],
        keyed: &'a [Keyed],
        cuts: &'a HashMap<String, Cut>,
    ) -> Ask<'a> {
        Ask {
            from,
            to,
            points,
            solo: None,
            bypass_fx: HashSet::new(),
            keyed,
            fresh: false,
            cuts,
            exact: false,
            meters: &[],
            master: false,
            cold: false,
            more: &[],
        }
    }
}

/// Another hearing in one render ([`Ask::more`]): its points, read with
/// its own cuts and bypasses (on top of the ask's solo).
pub struct Pass<'a> {
    pub points: &'a [ProbeAt],
    pub cuts: &'a HashMap<String, Cut>,
    pub bypass_fx: HashSet<String>,
    pub meters: &'a [String],
    pub master: bool,
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

    /// Two hearings' answers as one: replayed only if both were, rendered
    /// if either was everything.
    #[must_use]
    pub const fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Replayed, Self::Replayed) => Self::Replayed,
            (Self::Rendered, _) | (_, Self::Rendered) => Self::Rendered,
            _ => Self::Partly,
        }
    }
}

/// One hearing of a render, as heard.
pub struct PassHeard {
    /// Each point's signal, exactly `from`..`to`, interleaved stereo.
    pub signals: Vec<Vec<f32>>,
    /// How late each point was before it was lined up, samples.
    pub latencies: Vec<u32>,
    /// The master over `from`..`to`, when asked for.
    pub master: Option<Vec<f32>>,
    /// The song's latency at the master, samples.
    pub master_latency: u32,
    pub meters: HashMap<String, Vec<serde_json::Value>>,
    pub answered: Answered,
}

/// What a render heard.
pub struct Heard {
    /// Each point's signal, exactly `from`..`to`, interleaved stereo.
    pub signals: Vec<Vec<f32>>,
    /// How late each point was before it was lined up, samples.
    pub latencies: Vec<u32>,
    /// Every hearing's answer, as one.
    pub answered: Answered,
    /// The tracks rendered and played back, by guid (over every hearing),
    /// and the song's count.
    pub rendered: Vec<String>,
    pub replayed: Vec<String>,
    pub song_tracks: u32,
    /// Plugins run: copies made fresh, and copies reused.
    pub copied: u32,
    pub reused: u32,
    /// Plugins that would not be copied (rendered dry), and why.
    pub failures: Vec<String>,
    /// The chunks it was rendered in, side by side (0 when replayed), and
    /// the hearings ([`Ask::more`] and the first) that rendered anything.
    pub chunks: u32,
    pub passes: u32,
    /// Each of [`Ask::meters`]' instances' `SIGNAL_METERS` summary, one per
    /// chunk, over the chunk's part of `from`..`to` (none when replayed).
    pub meters: HashMap<String, Vec<serde_json::Value>>,
    /// The master ([`Ask::master`]) over `from`..`to`, and its latency.
    pub master: Option<Vec<f32>>,
    pub master_latency: u32,
    /// [`Ask::more`]'s hearings, in order.
    pub more: Vec<PassHeard>,
    /// What the recordings kept between listens hold after this one.
    pub cache: CacheStats,
    pub plan_ms: f64,
    pub copy_ms: f64,
    pub prime_ms: f64,
    pub blocks_ms: f64,
    pub ms: f64,
}

impl Heard {
    /// Span seconds rendered per second of the render (how many times
    /// faster than real time); `None` when nothing took any time.
    #[must_use]
    pub fn realtime_factor(&self, seconds: f64) -> Option<f64> {
        (self.ms > 0.0).then(|| seconds / (self.ms / 1e3))
    }

    /// Record what the render cost on `span`, which declares the
    /// [`render_span!`](crate::render_span) fields ([`render`] records
    /// itself on the current span).
    pub fn record(&self, span: &tracing::Span, points: usize, seconds: f64) {
        span.record("render.points", points);
        span.record("render.seconds", seconds);
        span.record("render.answered", self.answered.word());
        span.record("render.tracks_rendered", self.rendered.len());
        span.record("render.tracks_replayed", self.replayed.len());
        span.record("render.passes", self.passes);
        span.record("render.chunks", self.chunks);
        span.record("render.plugins_copied", self.copied);
        span.record("render.plugins_reused", self.reused);
        span.record("render.copy_ms", self.copy_ms);
        span.record("render.blocks_ms", self.blocks_ms);
        span.record("render.ms", self.ms);
        if let Some(rtf) = self.realtime_factor(seconds) {
            span.record("render.realtime_factor", rtf);
        }
        span.record("render.cache_bytes", self.cache.bytes);
        span.record("render.cache_recordings", self.cache.recordings);
    }
}

/// A span declaring the fields [`Heard::record`] fills, with the action's
/// own after them.
#[macro_export]
macro_rules! render_span {
    ($name:expr) => {
        tracing::info_span!(
            $name,
            render.points = tracing::field::Empty,
            render.seconds = tracing::field::Empty,
            render.answered = tracing::field::Empty,
            render.tracks_rendered = tracing::field::Empty,
            render.tracks_replayed = tracing::field::Empty,
            render.passes = tracing::field::Empty,
            render.chunks = tracing::field::Empty,
            render.plugins_copied = tracing::field::Empty,
            render.plugins_reused = tracing::field::Empty,
            render.copy_ms = tracing::field::Empty,
            render.blocks_ms = tracing::field::Empty,
            render.ms = tracing::field::Empty,
            render.realtime_factor = tracing::field::Empty,
            render.cache_bytes = tracing::field::Empty,
            render.cache_recordings = tracing::field::Empty
        )
    };
    ($name:expr, $($rest:tt)+) => {
        tracing::info_span!(
            $name,
            render.points = tracing::field::Empty,
            render.seconds = tracing::field::Empty,
            render.answered = tracing::field::Empty,
            render.tracks_rendered = tracing::field::Empty,
            render.tracks_replayed = tracing::field::Empty,
            render.passes = tracing::field::Empty,
            render.chunks = tracing::field::Empty,
            render.plugins_copied = tracing::field::Empty,
            render.plugins_reused = tracing::field::Empty,
            render.copy_ms = tracing::field::Empty,
            render.blocks_ms = tracing::field::Empty,
            render.ms = tracing::field::Empty,
            render.realtime_factor = tracing::field::Empty,
            render.cache_bytes = tracing::field::Empty,
            render.cache_recordings = tracing::field::Empty,
            $($rest)+
        )
    };
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

/// Where a copy is kept warm: its FX, and for a copy cut at a block, the
/// cut (a cut copy is never an uncut one).
fn warm_key(guid: &str, cut: Option<&Cut>) -> String {
    match cut {
        None => guid.to_owned(),
        Some(c) => format!(
            "{guid}#{}:{}",
            c.block,
            match c.side {
                Side::Pre => "pre",
                Side::Post => "post",
            }
        ),
    }
}

/// The recordings' clock: a recording heard is stamped again, so the one
/// heard longest ago goes first.
static CLOCK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tick() -> u64 {
    CLOCK.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
}

/// One point of a track as a render heard it.
struct Recorded {
    key: u64,
    /// Rendered in one piece ([`Ask::exact`]).
    exact: bool,
    /// Rendered on copies made for it ([`Ask::cold`]).
    cold: bool,
    /// Render frames `start..end`.
    start: u64,
    end: u64,
    samples: Arc<[f32]>,
    latency: u32,
    /// When it was last kept or heard: the oldest goes first.
    stamp: std::sync::atomic::AtomicU64,
}

impl Recorded {
    /// Whether it is what `key` sounds like over render frames
    /// `start..to` (and as late as it was past `to`).
    fn holds(&self, key: u64, (start, to): (u64, u64)) -> bool {
        self.key == key && self.start <= start && self.end >= to + u64::from(self.latency)
    }

    /// Whether it is good enough for a render asking `exact` and `cold`.
    fn serves(&self, (exact, cold): (bool, bool)) -> bool {
        (self.exact || !exact) && (self.cold || !cold)
    }

    fn bytes(&self) -> u64 {
        (self.samples.len() * std::mem::size_of::<f32>()) as u64
    }
}

/// A recording's address: a track (or [`MASTER`]), and where in it.
type Point = (String, Probe);

/// The master's recordings' track.
const MASTER: &str = "\u{0}master";

/// How much the recordings may hold at most unless set
/// (`STUDIO_LISTEN_CACHE_MB`, or `LISTEN_CACHE {limit_mb}`): 32 GiB.
const DEFAULT_LIMIT_MB: u64 = 32 * 1024;

/// What the recordings kept between listens hold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub recordings: u64,
    pub bytes: u64,
    pub limit_bytes: u64,
    /// Plugin copies kept warm.
    pub warm: u64,
    /// Recordings let go to stay under the limit, since the engine began.
    pub evicted: u64,
}

/// What listens keep between calls. Held for a whole render: two listens
/// never run at once (their copies share Signal sessions).
#[derive(Default)]
struct Kept {
    /// Per hearing of a render (the first, then [`Ask::more`]'s), per
    /// chunk, by [`warm_key`].
    warm: Vec<Vec<HashMap<String, Warm>>>,
    /// By point and key: a point keeps what it sounded like before and
    /// after a change, so putting a change back replays the first.
    recordings: HashMap<(Point, u64), Recorded>,
    bytes: u64,
    /// Set by [`set_limit`]; else the default.
    limit: Option<u64>,
    evicted: u64,
}

/// The limit `STUDIO_LISTEN_CACHE_MB` sets, else the default, bytes.
fn default_limit() -> u64 {
    static LIMIT: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *LIMIT.get_or_init(|| {
        std::env::var("STUDIO_LISTEN_CACHE_MB")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_LIMIT_MB)
            * 1024
            * 1024
    })
}

impl Kept {
    fn limit(&self) -> u64 {
        self.limit.unwrap_or_else(default_limit)
    }

    /// Keep a recording — unless one under the same key already covers
    /// what it covers, as well made, which is then only stamped — and let
    /// the ones heard longest ago go while they hold more than the limit.
    fn keep(&mut self, at: Point, r: Recorded) {
        let slot = (at, r.key);
        if let Some(old) = self.recordings.get(&slot)
            && old.start <= r.start
            && old.end >= r.end
            && old.serves((r.exact, r.cold))
        {
            old.stamp
                .store(tick(), std::sync::atomic::Ordering::Relaxed);
            return;
        }
        r.stamp.store(tick(), std::sync::atomic::Ordering::Relaxed);
        self.bytes += r.bytes();
        if let Some(old) = self.recordings.insert(slot, r) {
            self.bytes -= old.bytes();
        }
        self.shrink();
    }

    /// Let the recordings heard longest ago go until they hold at most the
    /// limit.
    fn shrink(&mut self) {
        let limit = self.limit();
        if self.bytes <= limit {
            return;
        }
        let mut by_age: Vec<(u64, (Point, u64))> = self
            .recordings
            .iter()
            .map(|(k, r)| {
                (
                    r.stamp.load(std::sync::atomic::Ordering::Relaxed),
                    k.clone(),
                )
            })
            .collect();
        by_age.sort_unstable_by_key(|(stamp, _)| *stamp);
        for (_, k) in by_age {
            if self.bytes <= limit {
                break;
            }
            if let Some(r) = self.recordings.remove(&k) {
                self.bytes -= r.bytes();
                self.evicted += 1;
            }
        }
    }

    /// The recording of `at` under `key` over `frames`, good enough for a
    /// render asking `need` (`exact`, `cold`), if one is kept: stamped as
    /// heard.
    fn holding(
        &self,
        at: &Point,
        (key, need): (Option<u64>, (bool, bool)),
        frames: (u64, u64),
    ) -> Option<&Recorded> {
        let key = key?;
        let r = self
            .recordings
            .get(&(at.clone(), key))
            .filter(|r| r.holds(key, frames) && r.serves(need))?;
        r.stamp.store(tick(), std::sync::atomic::Ordering::Relaxed);
        Some(r)
    }

    fn stats(&self) -> CacheStats {
        CacheStats {
            recordings: self.recordings.len() as u64,
            bytes: self.bytes,
            limit_bytes: self.limit(),
            warm: self.warm.iter().flatten().map(|w| w.len() as u64).sum(),
            evicted: self.evicted,
        }
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

/// At most this many threads make copies (or read live states) at once,
/// per chunk, unless `STUDIO_LISTEN_COPY_THREADS` says: one, since making
/// Signal copies side by side measured no faster on Unbreakable's kit (they
/// wait on each other).
const COPY_THREADS: usize = 1;

fn copy_threads() -> usize {
    static N: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *N.get_or_init(|| {
        std::env::var("STUDIO_LISTEN_COPY_THREADS")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(COPY_THREADS)
    })
}

/// `f` over `items` on up to [`copy_threads`] threads, the answers in
/// `items`' order.
fn side_by_side<T: Send, R: Send>(items: Vec<T>, f: impl Fn(T) -> R + Sync) -> Vec<R> {
    if items.len() <= 1 || copy_threads() == 1 {
        return items.into_iter().map(f).collect();
    }
    let n = items.len();
    let queue = Mutex::new(items.into_iter().enumerate().collect::<Vec<_>>());
    let done: Mutex<Vec<Option<R>>> = Mutex::new((0..n).map(|_| None).collect());
    std::thread::scope(|s| {
        for _ in 0..copy_threads().min(n) {
            s.spawn(|| {
                loop {
                    let next = queue
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .pop();
                    let Some((i, item)) = next else { break };
                    let r = f(item);
                    done.lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)[i] = Some(r);
                }
            });
        }
    });
    done.into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .into_iter()
        .flatten()
        .collect()
}

/// The live plugins on `tracks`' chains, by track, their states read (side
/// by side: each read holds only its own plugin's lock).
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
    let wanted: Vec<(String, String, String, u64)> = chains
        .into_iter()
        .flat_map(|(track, fx)| {
            let names = &names;
            fx.into_iter().filter_map(move |(g, held)| {
                Some((track.clone(), g.clone(), names.get(&g)?.clone(), held))
            })
        })
        .collect();
    let read = side_by_side(wanted, |(track, guid, name, held)| {
        let state = daw.save_plugin_state(&guid).unwrap_or_default();
        (
            track,
            Instance {
                guid,
                name,
                state,
                held,
            },
        )
    });
    let mut out: HashMap<String, Vec<Instance>> = HashMap::new();
    for (track, i) in read {
        out.entry(track).or_default().push(i);
    }
    out
}

/// What [`ready`] made ready.
struct Ready {
    plugins: Arc<Plugins>,
    /// Each copy's guid, where it is kept warm, and the state and held
    /// values it carries.
    kept: Vec<(String, String, u64, u64)>,
    copied: u32,
    reused: u32,
    failures: Vec<String>,
}

/// A Signal instance's state given the session `tag` of its own; any
/// other's as it is.
fn restated(name: &str, state: Vec<u8>, tag: &str) -> Vec<u8> {
    if crate::signal_kit::is_signal_bundle(name) {
        crate::signal_kit::private_session(state, tag)
    } else {
        state
    }
}

/// A copy of `i` made afresh, in Signal session `tag`, cut at `cut`.
fn copy_of(
    daw: &Standalone,
    i: &Instance,
    (tag, cut): (&str, Option<&Cut>),
) -> Result<Box<dyn PluginInstance>, String> {
    let restate = |name: &str, state: Vec<u8>| restated(name, state, tag);
    let how = Copying {
        make: Some(&crate::mix_measure::make_fresh),
        restate: Some(&restate),
    };
    let (mut copy, _) = copy_plugin(daw, &i.name, i.state.clone(), how)?;
    if let Some(cut) = cut {
        cut_copy(&mut *copy, cut)?;
    }
    Ok(copy)
}

/// A copy of each of `wanted` carrying its live instance's state: the warm
/// one when its state is unchanged (reset, so it starts as from silence;
/// never with `cold`), else made again — those side by side. A Signal
/// instance in `cuts` is cut at its block (its copies kept apart).
fn ready(
    daw: &Standalone,
    wanted: Vec<Instance>,
    warm: &mut HashMap<String, Warm>,
    (cuts, tag, cold): (&HashMap<String, Cut>, &str, bool),
) -> Result<Ready, String> {
    let plugins = Arc::new(Plugins::default());
    let mut out = Ready {
        plugins: plugins.clone(),
        kept: Vec::new(),
        copied: 0,
        reused: 0,
        failures: Vec::new(),
    };
    let restate = |name: &str, state: Vec<u8>| restated(name, state, tag);
    let mut to_make: Vec<(Instance, String, Option<Cut>)> = Vec::new();
    for i in wanted {
        let cut = cuts.get(&i.guid).cloned();
        let at = warm_key(&i.guid, cut.as_ref());
        let print = fingerprint(&i.state);
        let usable = warm.remove(&at).and_then(|mut w| {
            if cold || w.state != print {
                drop_later(vec![w.plugin]);
                return None;
            }
            if w.held != i.held {
                // Its state loaded again undoes a cut: cut it again.
                let again = reload(&mut *w.plugin, &i, &restate)
                    && cut
                        .as_ref()
                        .is_none_or(|c| cut_copy(&mut *w.plugin, c).is_ok());
                if !again {
                    drop_later(vec![w.plugin]);
                    return None;
                }
            }
            Some(w)
        });
        match usable {
            Some(mut w) => {
                if !w.plugin.reset() {
                    w.plugin.deactivate();
                }
                out.reused += 1;
                // The song's placed hits as the live instance has them now
                // (a warm copy's may be an edit step old).
                crate::clap_host::hand_copy(daw, &i.guid, &*w.plugin);
                plugins.write().insert(i.guid.clone(), w.plugin);
                out.kept.push((i.guid, at, print, i.held));
            }
            None => to_make.push((i, at, cut)),
        }
    }
    let made = side_by_side(to_make, |(i, at, cut)| {
        let copy = copy_of(daw, &i, (tag, cut.as_ref()));
        (i, at, cut.is_some(), copy)
    });
    for (i, at, was_cut, copy) in made {
        match copy {
            Ok(copy) => {
                crate::clap_host::hand_copy(daw, &i.guid, &*copy);
                plugins.write().insert(i.guid.clone(), copy);
                out.copied += 1;
                let print = fingerprint(&i.state);
                out.kept.push((i.guid, at, print, i.held));
            }
            // A point inside a cut instance cannot be heard without it.
            Err(why) if was_cut => return Err(why),
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

/// Each track's own fingerprint: as a render reads it (`prints`), its
/// plugins' states and cuts, and which tracks the render solos (`solo`: a
/// solo anywhere silences what is not soloed, though its own print does
/// not move).
fn own_prints(
    prints: Vec<(String, u64)>,
    plugins: &HashMap<String, Vec<Instance>>,
    (cuts, solo): (&HashMap<String, Cut>, u64),
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
            solo.hash(&mut h);
            (g, h.finish())
        })
        .collect()
}

/// A fingerprint of which tracks a render solos: `solo` when it says,
/// else the song's own solos; 0 when none is.
fn solo_print(solo: Option<&HashSet<String>>, all: &[daw_proto::Track]) -> u64 {
    let mut soloed: Vec<&str> = match solo {
        Some(s) => s.iter().map(String::as_str).collect(),
        None => all
            .iter()
            .filter(|t| t.soloed)
            .map(|t| t.guid.as_str())
            .collect(),
    };
    if soloed.is_empty() {
        return 0;
    }
    soloed.sort_unstable();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    soloed.hash(&mut h);
    h.finish()
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

/// One hearing of a render: the first ([`Ask`]'s own) or one of
/// [`Ask::more`].
struct Spec<'a> {
    points: &'a [ProbeAt],
    cuts: &'a HashMap<String, Cut>,
    bypass: &'a HashSet<String>,
    meters: &'a [String],
    master: bool,
}

/// What one hearing will render, and what it has to.
struct Job {
    /// The hearing, by index in the specs.
    spec: usize,
    fed: HashMap<String, Fed>,
    wanted: Vec<Instance>,
    /// The points, then the master's feeders when the master is not
    /// recorded, then every rendered track's output to keep.
    probes: Vec<ProbeAt>,
    only: HashSet<String>,
    rendered: HashSet<String>,
}

/// One hearing's plan.
struct Planned {
    plan: Plan,
    keys: HashMap<String, u64>,
    master_key: u64,
}

/// The tracks that sum into the master: top-level (or whose folder is not
/// the song's), sending to their parent.
fn master_feeders(all: &[daw_proto::Track]) -> Vec<String> {
    let guids: HashSet<&str> = all.iter().map(|t| t.guid.as_str()).collect();
    all.iter()
        .filter(|t| t.parent_send)
        .filter(|t| t.parent_guid.as_deref().is_none_or(|p| !guids.contains(p)))
        .map(|t| t.guid.clone())
        .collect()
}

/// The master's key: its fader, pan and mute, the solos, and the keys of
/// what sums into it.
fn master_key(own: u64, feeders: &[String], keys: &HashMap<String, u64>) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    MASTER.hash(&mut h);
    own.hash(&mut h);
    for f in feeders {
        keys.get(f).copied().unwrap_or(0).hash(&mut h);
    }
    h.finish()
}

/// Render `ask` of song `song` (`project` its context): what feeds its
/// points, played back where nothing it hears has changed; its other
/// hearings ([`Ask::more`]) side by side with it.
///
/// # Errors
/// The span is empty or too long, or a cut instance would not be copied.
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
    let specs: Vec<Spec<'_>> = std::iter::once(Spec {
        points: ask.points,
        cuts: ask.cuts,
        bypass: &ask.bypass_fx,
        meters: ask.meters,
        master: ask.master,
    })
    .chain(ask.more.iter().map(|p| Spec {
        points: p.points,
        cuts: p.cuts,
        bypass: &p.bypass_fx,
        meters: p.meters,
        master: p.master,
    }))
    .collect();
    let all = daw::service::Tracks::all(daw, project.clone());
    let routes = Routes::new(
        all.iter()
            .map(|t| (t.guid.as_str(), t.parent_guid.as_deref(), t.parent_send)),
        sends_into(daw, project, &all),
    );
    let feeders = master_feeders(&all);
    let mut roots: Vec<String> = specs
        .iter()
        .flat_map(|s| s.points.iter().map(|p| p.track.clone()))
        .collect();
    if specs.iter().any(|s| s.master) {
        roots.extend(feeders.iter().cloned());
    }
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
    let solo = solo_print(ask.solo.as_ref(), &all);
    let master_own = daw
        .read_project(song, |p| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (
                p.master_volume.to_bits(),
                p.master_pan.to_bits(),
                p.master_muted,
                solo,
            )
                .hash(&mut h);
            h.finish()
        })
        .unwrap_or(0);
    // The song as each hearing reads it: its solos and bypasses.
    let mut prints_by_bypass: HashMap<Vec<String>, Vec<(String, u64)>> = HashMap::new();
    let keyed: Vec<(HashMap<String, u64>, u64)> = specs
        .iter()
        .map(|s| {
            let mut by: Vec<String> = s.bypass.iter().cloned().collect();
            by.sort_unstable();
            let prints = prints_by_bypass
                .entry(by)
                .or_insert_with(|| {
                    ProjectRenderer::offline(
                        daw,
                        song,
                        RATE,
                        Offline {
                            solo: ask.solo.clone(),
                            bypass_fx: s.bypass.clone(),
                            ..Offline::default()
                        },
                    )
                    .track_prints()
                })
                .clone();
            let keys = keys(
                &routes,
                &everything,
                (&own_prints(prints, &live, (s.cuts, solo)), &sources),
            );
            let m = master_key(master_own, &feeders, &keys);
            (keys, m)
        })
        .collect();
    let (start, from, to) = frames_of(ask.from, ask.to);
    let need = (ask.exact, ask.cold);
    let mut kept = kept()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let sources_of = |g: &str| match sources.get(g) {
        Some(known) => known.clone(),
        None => Some(Vec::new()),
    };
    let master_at: Point = (MASTER.to_owned(), Probe::PostFader);
    // Each hearing planned against what is kept; its points, then the
    // master's feeders when the master has to be rendered.
    let mut planned: Vec<Planned> = Vec::new();
    let mut points_of: Vec<Vec<ProbeAt>> = Vec::new();
    for (s, spec) in specs.iter().enumerate() {
        let (keys, mkey) = keyed[s].clone();
        let key = |g: &str| keys.get(g).copied();
        let mut points = spec.points.to_vec();
        let master_held = spec.master
            && kept
                .holding(&master_at, (Some(mkey), need), (start, to))
                .is_some();
        let synthetic: HashSet<ProbeAt> = if spec.master && !master_held {
            feeders
                .iter()
                .map(|g| ProbeAt {
                    track: g.clone(),
                    at: Probe::PostFader,
                })
                .collect()
        } else {
            HashSet::new()
        };
        points.extend(synthetic.iter().cloned());
        let plan = if ask.fresh {
            Plan {
                answered: Answered::Rendered,
                rendered: everything.clone(),
                fed: HashSet::new(),
            }
        } else {
            plan(
                &routes,
                (&points, &everything),
                |p| {
                    !synthetic.contains(p)
                        && kept
                            .holding(&(p.track.clone(), p.at), (key(&p.track), need), (start, to))
                            .is_some()
                },
                |g| {
                    !routes.taps_before_its_fader(g)
                        && kept
                            .holding(
                                &(g.to_owned(), Probe::PostFader),
                                (key(g), need),
                                (start, to),
                            )
                            .is_some()
                },
                sources_of,
            )
        };
        planned.push(Planned {
            plan,
            keys,
            master_key: mkey,
        });
        points_of.push(points);
    }
    // What each hearing that has anything to render renders.
    let jobs: Vec<Job> = planned
        .iter()
        .enumerate()
        .filter(|(_, p)| p.plan.answered != Answered::Replayed)
        .map(|(s, p)| {
            let key = |g: &str| p.keys.get(g).copied();
            let fed: HashMap<String, Fed> = p
                .plan
                .fed
                .iter()
                .filter_map(|g| {
                    let r =
                        kept.holding(&(g.clone(), Probe::PostFader), (key(g), need), (start, to))?;
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
            let wanted: Vec<Instance> = p
                .plan
                .rendered
                .iter()
                .filter_map(|g| live.get(g))
                .flatten()
                .cloned()
                .collect();
            let mut probes = points_of[s].clone();
            probes.extend(p.plan.rendered.iter().map(|g| ProbeAt {
                track: g.clone(),
                at: Probe::PostFader,
            }));
            let mut only = p.plan.rendered.clone();
            only.extend(p.plan.fed.iter().cloned());
            Job {
                spec: s,
                fed,
                wanted,
                probes,
                only,
                rendered: p.plan.rendered.clone(),
            }
        })
        .collect();
    let plan_ms = began.elapsed().as_secs_f64() * 1e3;
    let spans = if ask.exact {
        vec![(start, start, to)]
    } else {
        chunks(start, from, to)
    };
    if kept.warm.len() < specs.len() {
        kept.warm.resize_with(specs.len(), Vec::new);
    }
    for pool in &mut kept.warm {
        if pool.len() < spans.len() {
            pool.resize_with(spans.len(), HashMap::new);
        }
    }
    let chunk_count = spans.len();
    // Every chunk of every hearing that renders, side by side.
    let outs: Vec<(usize, Vec<Result<ChunkOut, String>>)> = std::thread::scope(|sc| {
        let mut pools: Vec<Option<&mut Vec<HashMap<String, Warm>>>> =
            kept.warm.iter_mut().map(Some).collect();
        let running: Vec<(usize, Vec<_>)> = jobs
            .iter()
            .map(|job| {
                let spec = &specs[job.spec];
                let pool = pools[job.spec].take().expect("one job a hearing");
                let shared = Shared {
                    daw,
                    song,
                    from: ask.from,
                    to: ask.to,
                    solo: ask.solo.clone(),
                    bypass: spec.bypass.clone(),
                    cuts: spec.cuts,
                    meters: spec.meters,
                    cold: ask.cold,
                    master: spec.master,
                    slot: job.spec,
                    probes: &job.probes,
                    only: &job.only,
                    fed: &job.fed,
                };
                let threads = spans
                    .iter()
                    .zip(pool.iter_mut())
                    .enumerate()
                    .map(|(k, (span, warm))| {
                        let wanted = job.wanted.clone();
                        let last = k + 1 == chunk_count;
                        let shared = shared.clone();
                        sc.spawn(move || run_chunk(&shared, ((k, last), *span), wanted, warm))
                    })
                    .collect::<Vec<_>>();
                (job.spec, threads)
            })
            .collect();
        running
            .into_iter()
            .map(|(s, threads)| {
                (
                    s,
                    threads
                        .into_iter()
                        .map(|t| {
                            t.join()
                                .unwrap_or_else(|_| Err("a chunk's render panicked".to_owned()))
                        })
                        .collect(),
                )
            })
            .collect()
    });
    let mut rendered_outs: HashMap<usize, Vec<ChunkOut>> = HashMap::new();
    for (s, chunk_outs) in outs {
        rendered_outs.insert(s, chunk_outs.into_iter().collect::<Result<_, _>>()?);
    }
    let covered = |r: &[f32]| start + (r.len() / 2) as u64;
    let made_exact = ask.exact || spans.len() == 1;
    let mut heard_passes: Vec<PassHeard> = Vec::new();
    let (mut rendered_all, mut fed_all): (HashSet<String>, HashSet<String>) = Default::default();
    let (mut copied, mut reused, mut failures) = (0u32, 0u32, Vec::new());
    let (mut copy_ms, mut prime_ms, mut blocks_ms) = (0.0f64, 0.0f64, 0.0f64);
    for (s, spec) in specs.iter().enumerate() {
        let p = &planned[s];
        let n = spec.points.len();
        let Some(outs) = rendered_outs.remove(&s) else {
            // Replayed: every point (and the master) from what is kept.
            let key = |g: &str| p.keys.get(g).copied();
            let (signals, latencies): (Vec<Vec<f32>>, Vec<u32>) = spec
                .points
                .iter()
                .filter_map(|pt| {
                    kept.holding(
                        &(pt.track.clone(), pt.at),
                        (key(&pt.track), need),
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
            let master = spec
                .master
                .then(|| {
                    kept.holding(&master_at, (Some(p.master_key), need), (start, to))
                        .map(|r| {
                            (
                                line_up(&r.samples, r.start, (from, to), r.latency),
                                r.latency,
                            )
                        })
                })
                .flatten();
            fed_all.extend(spec.points.iter().map(|pt| pt.track.clone()));
            heard_passes.push(PassHeard {
                signals,
                latencies,
                master_latency: master.as_ref().map_or(0, |m| m.1),
                master: master.map(|m| m.0),
                meters: HashMap::new(),
                answered: Answered::Replayed,
            });
            continue;
        };
        let job = jobs.iter().find(|j| j.spec == s).expect("its job");
        // Each probe as rendered, from the preroll's start until it reached
        // `to`, late by its latency.
        // One chunk's recordings are kept as they are; chunks are joined.
        let raw: Vec<Arc<[f32]>> = (0..job.probes.len())
            .map(|i| match outs.as_slice() {
                [one] => one.raw[i].clone(),
                _ => outs.iter().flat_map(|o| o.raw[i].iter().copied()).collect(),
            })
            .collect();
        let latencies: Vec<u32> = outs
            .first()
            .map(|o| o.latencies.clone())
            .unwrap_or_default();
        let master_latency = outs.first().map_or(0, |o| o.master_latency);
        let signals: Vec<Vec<f32>> = raw[..n]
            .iter()
            .zip(&latencies)
            .map(|(r, late)| line_up(r, start, (from, to), *late))
            .collect();
        let master = spec.master.then(|| {
            let m: Vec<f32> = outs.iter().flat_map(|o| o.master.iter().copied()).collect();
            let lined = line_up(&m, start, (from, to), master_latency);
            kept.keep(
                master_at.clone(),
                Recorded {
                    key: p.master_key,
                    exact: made_exact,
                    cold: ask.cold,
                    start,
                    end: covered(&m),
                    samples: m.into(),
                    latency: master_latency,
                    stamp: 0.into(),
                },
            );
            lined
        });
        for ((pt, samples), late) in job.probes.iter().zip(raw).zip(&latencies) {
            if !job.rendered.contains(&pt.track) {
                continue;
            }
            let Some(key) = p.keys.get(&pt.track).copied() else {
                continue;
            };
            kept.keep(
                (pt.track.clone(), pt.at),
                Recorded {
                    key,
                    exact: made_exact,
                    cold: ask.cold,
                    start,
                    end: covered(&samples),
                    samples,
                    latency: *late,
                    stamp: 0.into(),
                },
            );
        }
        let mut meters: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
        for (guid, v) in outs.iter().flat_map(|o| o.meters.iter().cloned()) {
            meters.entry(guid).or_default().push(v);
        }
        copied += outs.iter().map(|o| o.copied).sum::<u32>();
        reused += outs.iter().map(|o| o.reused).sum::<u32>();
        failures.extend(outs.iter().flat_map(|o| o.failures.iter().cloned()));
        let most = |f: fn(&ChunkOut) -> f64| outs.iter().map(f).fold(0.0, f64::max);
        copy_ms = copy_ms.max(most(|o| o.copy_ms));
        prime_ms = prime_ms.max(most(|o| o.prime_ms));
        blocks_ms = blocks_ms.max(most(|o| o.blocks_ms));
        rendered_all.extend(p.plan.rendered.iter().cloned());
        fed_all.extend(p.plan.fed.iter().cloned());
        heard_passes.push(PassHeard {
            signals,
            latencies: latencies[..n].to_vec(),
            master,
            master_latency,
            meters,
            answered: p.plan.answered,
        });
    }
    failures.sort();
    failures.dedup();
    let cache = kept.stats();
    drop(kept);
    let answered = heard_passes
        .iter()
        .map(|h| h.answered)
        .reduce(Answered::and)
        .unwrap_or(Answered::Replayed);
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let mut rendered: Vec<String> = rendered_all.iter().cloned().collect();
    rendered.sort();
    let mut replayed: Vec<String> = fed_all.difference(&rendered_all).cloned().collect();
    replayed.sort();
    let mut passes = heard_passes.into_iter();
    let first = passes.next().expect("the first hearing");
    let heard = Heard {
        signals: first.signals,
        latencies: first.latencies,
        answered,
        rendered,
        replayed,
        song_tracks: count(all.len()),
        copied,
        reused,
        failures,
        chunks: if jobs.is_empty() {
            0
        } else {
            count(chunk_count)
        },
        passes: count(jobs.len()),
        meters: first.meters,
        master: first.master,
        master_latency: first.master_latency,
        more: passes.collect(),
        cache,
        plan_ms,
        copy_ms,
        prime_ms,
        blocks_ms,
        ms: began.elapsed().as_secs_f64() * 1e3,
    };
    // On the caller's span, when it declares `render_span!`'s fields.
    heard.record(
        &tracing::Span::current(),
        specs.iter().map(|s| s.points.len()).sum(),
        ask.to - ask.from,
    );
    Ok(heard)
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

/// What every chunk of one hearing reads.
#[derive(Clone)]
struct Shared<'a> {
    daw: &'a Standalone,
    song: &'a str,
    /// The span, seconds.
    from: f64,
    to: f64,
    solo: Option<HashSet<String>>,
    bypass: HashSet<String>,
    cuts: &'a HashMap<String, Cut>,
    meters: &'a [String],
    cold: bool,
    /// Also keep what reaches the master.
    master: bool,
    /// The hearing's index: its warm copies and its Signal sessions.
    slot: usize,
    probes: &'a [ProbeAt],
    only: &'a HashSet<String>,
    fed: &'a HashMap<String, Fed>,
}

/// What one chunk heard.
struct ChunkOut {
    /// Each probe as it came out, late by its latency, over the chunk's
    /// kept render frames (the first chunk's from its preroll's start, the
    /// last's on until every probe has reached the span's end).
    raw: Vec<Arc<[f32]>>,
    latencies: Vec<u32>,
    /// What reached the master over the same frames (empty unless asked),
    /// and how late.
    master: Vec<f32>,
    master_latency: u32,
    copied: u32,
    reused: u32,
    failures: Vec<String>,
    /// The hearing's meters' summaries over this chunk's part of the span.
    meters: Vec<(String, serde_json::Value)>,
    copy_ms: f64,
    prime_ms: f64,
    blocks_ms: f64,
}

/// Run `SIGNAL_METERS` with `args` on each of `guids`' copies in
/// `plugins`: each one's answer, by guid (a copy without `signal/1`, or
/// whose call failed, is left out).
fn signal_meters(
    plugins: &Plugins,
    guids: &[String],
    args: &serde_json::Value,
) -> Vec<(String, serde_json::Value)> {
    let mut map = plugins.write();
    guids
        .iter()
        .filter_map(|g| {
            let signal = map.get_mut(g)?.signal()?;
            let text = crate::clap_host::plugin_call(|| {
                signal.run_action("SIGNAL_METERS", &args.to_string())
            })
            .ok()?;
            Some((g.clone(), serde_json::from_str(&text).ok()?))
        })
        .collect()
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
    // The first hearing's sessions keep the names they always had.
    let tag = if c.slot == 0 {
        format!("{SESSION_TAG}-{k}")
    } else {
        format!("{SESSION_TAG}-{}-{k}", c.slot)
    };
    let ready = ready(c.daw, wanted, warm, (c.cuts, &tag, c.cold))?;
    let copy_ms = copying.elapsed().as_secs_f64() * 1e3;
    // The blocks' meters, over this chunk's part of the span (not its
    // preroll): set before priming, which resets the copies (a reset keeps
    // the span and starts the summary afresh).
    let (_, from_frame, _) = frames_of(c.from, c.to);
    let seconds = |f: u64| f as f64 / f64::from(RATE);
    if !c.meters.is_empty() {
        let _ = signal_meters(
            &ready.plugins,
            c.meters,
            &serde_json::json!({ "from": seconds(keep.max(from_frame)), "to": seconds(end) }),
        );
    }
    let renderer = ProjectRenderer::offline(
        c.daw,
        c.song,
        RATE,
        Offline {
            plugins: ready.plugins.clone(),
            bypass_fx: c.bypass.clone(),
            solo: c.solo.clone(),
            tap: None,
            only: Some(c.only.clone()),
            probes: c.probes.to_vec(),
            fed: c.fed.clone(),
            record: None,
        },
    );
    let mut renderer = renderer;
    let primed = web_time::Instant::now();
    let song_latency = renderer.prime(u32::try_from(BLOCK).unwrap_or(1_024));
    let latencies = renderer.probe_latencies();
    let prime_ms = primed.elapsed().as_secs_f64() * 1e3;
    let blocks = web_time::Instant::now();
    let master_latency = if c.master { song_latency } else { 0 };
    let last = end
        + u64::from(
            latencies
                .iter()
                .copied()
                .max()
                .unwrap_or(0)
                .max(master_latency),
        );
    // As rendered, not lined up: a recording played back later enters the
    // delay compensation exactly as this track did, so what it feeds is as
    // late (its plugins told the same song time) as in this render. Each
    // probe is recorded over the frames kept: the first chunk's from its
    // preroll's start, the last's on until every probe has reached the end.
    let (from, to) = (
        if k == 0 { first } else { keep },
        if last_chunk { last } else { end },
    );
    renderer.record(Record {
        start: from,
        frames: to - from,
    });
    let master = render_blocks(&renderer, (first, last), c.master);
    let raw = renderer.take_recordings();
    let blocks_ms = blocks.elapsed().as_secs_f64() * 1e3;
    drop(renderer);
    let meters = if c.meters.is_empty() {
        Vec::new()
    } else {
        signal_meters(&ready.plugins, c.meters, &serde_json::json!({}))
    };
    let master = if c.master {
        master
            .get(((from - first) * 2) as usize..((to - first) * 2) as usize)
            .map(<[f32]>::to_vec)
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    // A cold render's copies are not kept: no cold render reuses one, and
    // every Signal instance alive makes the next copies slower to make.
    if c.cold {
        drop_later(
            ready
                .kept
                .iter()
                .filter_map(|(guid, ..)| ready.plugins.write().remove(guid))
                .collect(),
        );
    }
    for (guid, at, state, held) in ready.kept.into_iter().filter(|_| !c.cold) {
        if let Some(plugin) = ready.plugins.write().remove(&guid) {
            warm.insert(
                at,
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
        master,
        master_latency,
        copied: ready.copied,
        reused: ready.reused,
        failures: ready.failures,
        meters,
        copy_ms,
        prime_ms,
        blocks_ms,
    })
}

/// The renderer's blocks over render frames `start..end` (its probes
/// recorded as it goes, [`Record`]), and (when `master`) what reached the
/// master, interleaved stereo.
fn render_blocks(renderer: &ProjectRenderer, (start, end): (u64, u64), master: bool) -> Vec<f32> {
    let frames = (end - start) as usize;
    let mut sum: Vec<f32> = Vec::with_capacity(if master { frames * 2 } else { 0 });
    let mut at = start;
    while at < end {
        let n = BLOCK.min((end - at) as usize);
        let block = renderer.render_block(at, n);
        if master {
            let got = &block.samples[..(n * 2).min(block.samples.len())];
            sum.extend_from_slice(got);
            sum.resize(sum.len() + (n * 2 - got.len()), 0.0);
        }
        at += n as u64;
    }
    sum
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
        u32::try_from(kept.warm.iter().flatten().map(HashMap::len).sum::<usize>())
            .unwrap_or(u32::MAX),
        u32::try_from(kept.recordings.len()).unwrap_or(u32::MAX),
    );
    drop_later(
        kept.warm
            .iter_mut()
            .flatten()
            .flat_map(|w| w.drain().map(|(_, w)| w.plugin))
            .collect(),
    );
    kept.recordings.clear();
    kept.bytes = 0;
    counts
}

/// What the recordings hold now (waits for a render running).
#[must_use]
pub fn cache_stats() -> CacheStats {
    kept()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .stats()
}

/// Bound the recordings to `bytes` (`None`: the default again), letting the
/// ones heard longest ago go now if they hold more.
pub fn set_limit(bytes: Option<u64>) -> CacheStats {
    let mut kept = kept()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    kept.limit = bytes;
    kept.shrink();
    kept.stats()
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
        let before = keys(&routes, &all, (&own, &Sources::new()));
        let mut changed = own.clone();
        changed.insert("in".into(), 2);
        let after = keys(&routes, &all, (&changed, &Sources::new()));
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

    fn recorded(key: u64, (start, end): (u64, u64), frames: usize) -> Recorded {
        Recorded {
            key,
            exact: true,
            cold: false,
            start,
            end,
            samples: vec![0.0f32; frames * 2].into(),
            latency: 0,
            stamp: 0.into(),
        }
    }

    #[test]
    fn the_recordings_heard_longest_ago_go_first_past_the_byte_limit() {
        // Room for two recordings of 100 frames (800 bytes each).
        let mut kept = Kept {
            limit: Some(1_600),
            ..Kept::default()
        };
        let at = |t: &str| (t.to_owned(), Probe::PostFader);
        kept.keep(at("a"), recorded(1, (0, 100), 100));
        kept.keep(at("b"), recorded(2, (0, 100), 100));
        assert_eq!(kept.bytes, 1_600);
        // `a` heard again: `b` is now the one heard longest ago.
        assert!(
            kept.holding(&at("a"), (Some(1), (false, false)), (0, 50))
                .is_some()
        );
        kept.keep(at("c"), recorded(3, (0, 100), 100));
        assert_eq!(kept.recordings.len(), 2);
        assert!(
            kept.holding(&at("b"), (Some(2), (false, false)), (0, 50))
                .is_none()
        );
        assert!(
            kept.holding(&at("a"), (Some(1), (false, false)), (0, 50))
                .is_some()
        );
        assert_eq!(kept.bytes, 1_600);
        assert_eq!(kept.stats().evicted, 1);
        // A shorter recording under a key that covers it already keeps the
        // longer one.
        kept.keep(at("a"), recorded(1, (10, 40), 30));
        assert!(
            kept.holding(&at("a"), (Some(1), (false, false)), (0, 100))
                .is_some()
        );
        // A warm-made one never serves a cold render.
        assert!(
            kept.holding(&at("a"), (Some(1), (true, true)), (0, 100))
                .is_none()
        );
    }

    #[test]
    fn a_solo_anywhere_moves_every_key() {
        let t = |g: &str, soloed: bool| daw_proto::Track {
            guid: g.into(),
            soloed,
            ..daw_proto::Track::default()
        };
        let none = [t("a", false), t("b", false)];
        let one = [t("a", true), t("b", false)];
        assert_eq!(solo_print(None, &none), 0);
        assert_ne!(solo_print(None, &one), 0);
        let asked: HashSet<String> = ["a".to_owned()].into();
        assert_eq!(solo_print(Some(&asked), &none), solo_print(None, &one));
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
