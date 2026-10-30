//! How the audio callback is keeping up, read every two seconds: the
//! block the device runs at, render time (mean and worst) against that
//! block's budget, blocks that overran it or that the device called
//! xruns — and the render ahead (the dual-buffer engine): how much is
//! rendered ahead of the playhead and how much it aims for, blocks that
//! found too little, restarts, edits that took it back, frames played
//! from before an edit while the edit's came, and what rendering ahead
//! costs against realtime. Read over MCP with `SESSION_AUDIO`.
//!
//! How much time the device went without audio from us is measured apart
//! from the render: frames delivered against the wall clock ([`Lag`]), so a
//! callback that blocked — or never ran — is a gap even though no block
//! was counted over budget. Every dropped block is exported as an
//! `audio.dropout` span at the time it happened, under one
//! `audio.dropouts` trace per interval, to line up with whatever else the
//! engine was doing then.
//! And what the audio sources hold (`memory`): distinct sources, bytes
//! mapped, streamed and decoded resident — also recorded on the global
//! OpenTelemetry meter as `session.audio.source_bytes` (by `kind`) and
//! `session.audio.sources`.
//!
//! Logged as `session_engine::audio`: a warning for every interval a block
//! missed (late, an xrun, or the tracks ahead played short); otherwise
//! one line when how it keeps up changes — the block size, the aim, the
//! load by a tenth — and once a minute while it does not.

use std::sync::Mutex;
use std::sync::atomic::Ordering::Relaxed;

use daw::standalone::audio_engine::EngineStats;
use daw::standalone::audio_engine::source_cache::{AudioMemory, audio_memory};
use facet::Facet;

/// The engine's counters at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Reading {
    pub calls: u64,
    pub frames: u64,
    pub render_ns: u64,
    pub over_budget: u64,
    pub xruns: u64,
    pub ahead_underruns: u64,
    pub ahead_restarts: u64,
    pub ahead_ns: u64,
    pub ahead_frames: u64,
    pub ahead_rewinds: u64,
    pub ahead_stale_frames: u64,
}

impl Reading {
    #[must_use]
    pub fn of(stats: &EngineStats) -> Self {
        Self {
            calls: stats.calls.load(Relaxed),
            frames: stats.frames_delivered.load(Relaxed),
            render_ns: stats.total_render_ns.load(Relaxed),
            over_budget: stats.over_budget.load(Relaxed),
            xruns: stats.xruns.load(Relaxed),
            ahead_underruns: stats.ahead_underruns.load(Relaxed),
            ahead_restarts: stats.ahead_restarts.load(Relaxed),
            ahead_ns: stats.ahead_render_ns.load(Relaxed),
            ahead_frames: stats.ahead_frames.load(Relaxed),
            ahead_rewinds: stats.ahead_rewinds.load(Relaxed),
            ahead_stale_frames: stats.ahead_stale_frames.load(Relaxed),
        }
    }
}

/// How an interval went.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct Health {
    /// Blocks rendered, and the device's block, frames.
    pub blocks: u64,
    pub frames: u32,
    /// A block's budget, and its render's mean and worst, ms.
    pub budget_ms: f64,
    pub mean_ms: f64,
    pub peak_ms: f64,
    /// Blocks over their budget, and blocks the device called xruns.
    pub over_budget: u64,
    pub xruns: u64,
    /// Time the device went without audio from us, ms ([`Lag`]): what a
    /// listener hears as a dropout, whatever the render's timing says.
    pub gap_ms: f64,
    /// Rendered ahead of the playhead now, ms (0 with the render ahead off
    /// or at rest), and how far it aims to be: its depth, less while it
    /// keeps up with time to spare or while edits come in.
    pub ahead_ms: f64,
    pub ahead_aim_ms: f64,
    /// Blocks that found less rendered ahead than they needed, and times
    /// the render ahead started again.
    pub ahead_underruns: u64,
    pub ahead_restarts: u64,
    /// Edits to tracks rendered ahead that took it back to just past the
    /// playhead, and the time played from before an edit while the
    /// edit's own frames came, ms (the edit heard that much late).
    pub ahead_rewinds: u64,
    pub ahead_stale_ms: f64,
    /// The render ahead's time over the audio it rendered: 0.25 is a
    /// quarter of a core's realtime; past 1 it cannot keep up.
    pub ahead_load: f64,
    /// The slowest render of the interval — a device block or a chunk
    /// rendered ahead — and where its time went.
    pub worst: Option<WorstBlock>,
    /// What the audio sources hold at the interval's end.
    pub memory: SourceMemory,
}

/// The slowest render of an interval, stage by stage, ms.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct WorstBlock {
    pub frames: u32,
    pub total_ms: f64,
    /// Getting the song's snapshot and the scratch: lock waits.
    pub snapshot_ms: f64,
    /// Waiting for the plugins.
    pub plugin_lock_ms: f64,
    /// Every track's items into its bus: sources read, stretched, faded.
    pub items_ms: f64,
    /// Every track's FX.
    pub fx_ms: f64,
    /// The track whose items took longest, and how long.
    pub slowest_items_track: String,
    pub slowest_items_ms: f64,
    /// The track whose FX took longest, and how long.
    pub slowest_fx_track: String,
    pub slowest_fx_ms: f64,
}

impl WorstBlock {
    /// `p` with its tracks named by `name` (a project track index).
    #[must_use]
    pub fn of(
        p: &daw::standalone::audio_engine::render::BlockProfile,
        name: impl Fn(u32) -> String,
    ) -> Self {
        let ms = |us: u32| f64::from(us) / 1e3;
        Self {
            frames: p.frames,
            total_ms: ms(p.total_us),
            snapshot_ms: ms(p.snapshot_us),
            plugin_lock_ms: ms(p.plugin_lock_us),
            items_ms: ms(p.items_us),
            fx_ms: ms(p.fx_us),
            slowest_items_track: name(p.slowest_items_track),
            slowest_items_ms: ms(p.slowest_items_us),
            slowest_fx_track: name(p.slowest_track),
            slowest_fx_ms: ms(p.slowest_track_us),
        }
    }
}

/// What the audio sources hold, MB.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct SourceMemory {
    /// Distinct sources loaded (one per recording, however many items
    /// are cut from it).
    pub sources: u64,
    /// Files mapped and their size: paged in around the playhead from the
    /// page cache, not the engine's own memory.
    pub mapped_files: u64,
    pub mapped_mb: f64,
    /// Proxies' decoded chunks, resident around the playhead.
    pub streamed_mb: f64,
    /// Media decoded whole, resident (the preload budget's count, shared
    /// with the sampler).
    pub resident_mb: f64,
}

impl SourceMemory {
    #[must_use]
    pub fn of(m: AudioMemory) -> Self {
        let mb = |bytes: u64| bytes as f64 / (1024.0 * 1024.0);
        Self {
            sources: m.sources,
            mapped_files: m.mapped_files,
            mapped_mb: mb(m.mapped_bytes),
            streamed_mb: mb(m.streamed_bytes),
            resident_mb: mb(m.resident_bytes),
        }
    }
}

/// Record `m` on the global meter: bytes by kind, and the sources.
fn record(m: AudioMemory) {
    use opentelemetry::KeyValue;
    struct Gauges {
        bytes: opentelemetry::metrics::Gauge<u64>,
        sources: opentelemetry::metrics::Gauge<u64>,
    }
    static GAUGES: std::sync::OnceLock<Gauges> = std::sync::OnceLock::new();
    let gauges = GAUGES.get_or_init(|| {
        let meter = opentelemetry::global::meter("session_engine");
        Gauges {
            bytes: meter
                .u64_gauge("session.audio.source_bytes")
                .with_unit("By")
                .with_description(
                    "What the audio sources hold, by kind: mapped, streamed, resident",
                )
                .build(),
            sources: meter
                .u64_gauge("session.audio.sources")
                .with_description("Distinct audio sources loaded (one per recording)")
                .build(),
        }
    });
    for (kind, bytes) in [
        ("mapped", m.mapped_bytes),
        ("streamed", m.streamed_bytes),
        ("resident", m.resident_bytes),
    ] {
        gauges.bytes.record(bytes, &[KeyValue::new("kind", kind)]);
    }
    gauges.sources.record(m.sources, &[]);
}

/// What a sample count is in ms at `rate`.
fn ms(frames: f64, rate: u32) -> f64 {
    frames * 1000.0 / f64::from(rate.max(1))
}

impl Health {
    /// The interval from `before` to `now`, with the block size, the worst
    /// render and the fill read at its end.
    #[must_use]
    pub fn between(before: Reading, now: Reading, at_end: AtEnd, rate: u32) -> Self {
        let blocks = now.calls.saturating_sub(before.calls);
        let ahead_frames = now.ahead_frames.saturating_sub(before.ahead_frames);
        let ahead_ns = now.ahead_ns.saturating_sub(before.ahead_ns);
        Self {
            blocks,
            frames: at_end.frames,
            budget_ms: ms(f64::from(at_end.frames), rate),
            mean_ms: if blocks > 0 {
                now.render_ns.saturating_sub(before.render_ns) as f64 / blocks as f64 / 1e6
            } else {
                0.0
            },
            peak_ms: at_end.peak_ns as f64 / 1e6,
            over_budget: now.over_budget.saturating_sub(before.over_budget),
            xruns: now.xruns.saturating_sub(before.xruns),
            gap_ms: 0.0,
            ahead_ms: ms(at_end.ahead_fill as f64, rate),
            ahead_aim_ms: ms(at_end.ahead_aim as f64, rate),
            ahead_underruns: now.ahead_underruns.saturating_sub(before.ahead_underruns),
            ahead_restarts: now.ahead_restarts.saturating_sub(before.ahead_restarts),
            ahead_rewinds: now.ahead_rewinds.saturating_sub(before.ahead_rewinds),
            ahead_stale_ms: ms(
                now.ahead_stale_frames
                    .saturating_sub(before.ahead_stale_frames) as f64,
                rate,
            ),
            ahead_load: if ahead_frames > 0 {
                ahead_ns as f64 / 1e9 / (ahead_frames as f64 / f64::from(rate.max(1)))
            } else {
                0.0
            },
            worst: None,
            memory: SourceMemory::default(),
        }
    }

    /// Whether a block was late, the device went without audio, or a
    /// block played short of the tracks ahead: the interval was not
    /// smooth.
    #[must_use]
    pub fn missed(&self) -> bool {
        self.over_budget > 0 || self.xruns > 0 || self.gap_ms > 0.0 || self.ahead_underruns > 0
    }

    /// How it keeps up, in the terms a line is logged for when they
    /// change: the block, the aim (to the ms) and the load (to a tenth).
    #[must_use]
    pub fn gist(&self) -> (u32, i64, i64) {
        (
            self.frames,
            self.ahead_aim_ms.round() as i64,
            (self.ahead_load * 10.0).round() as i64,
        )
    }
}

/// Whether to log an interval that kept up: its gist changed since the last
/// line, or `quiet` intervals have passed without one.
#[must_use]
pub fn worth_a_line(last: Option<(u32, i64, i64)>, now: &Health, since_line: u32) -> bool {
    last != Some(now.gist()) || since_line >= QUIET_INTERVALS
}

/// Intervals (of two seconds) between lines while nothing changes: a
/// minute.
const QUIET_INTERVALS: u32 = 30;

/// How far the audio delivered has fallen behind the wall clock since the
/// stream started, and the most it had before.
///
/// In a smooth stream the lag only wanders by about a block (where in a
/// block each reading lands); a dropout raises it for good — the time the
/// device played nothing is never made up. So a gap is the lag rising past
/// the most it had by more than that wander.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lag {
    lag_ms: f64,
    most_ms: f64,
}

impl Lag {
    /// One interval: `elapsed_ms` of wall clock, `delivered_ms` of audio.
    /// Returns the gap it added, ms. `wander_ms` is what a smooth stream
    /// may move by.
    pub fn step(&mut self, elapsed_ms: f64, delivered_ms: f64, wander_ms: f64) -> f64 {
        self.lag_ms += elapsed_ms - delivered_ms;
        let rose = self.lag_ms - self.most_ms;
        self.most_ms = self.most_ms.max(self.lag_ms);
        if rose > wander_ms { rose } else { 0.0 }
    }
}

/// What a smooth stream's lag may wander by at a block of `budget_ms`: two
/// blocks (a reading lands anywhere in one; the graph may run a larger
/// quantum than our block), never under 5 ms.
fn wander_ms(budget_ms: f64) -> f64 {
    (budget_ms * 2.0).max(5.0)
}

/// The stream state every backend reports while the device runs.
const STREAMING: i32 = 3;

/// What is read once, at an interval's end.
#[derive(Clone, Copy, Debug, Default)]
pub struct AtEnd {
    pub frames: u32,
    pub peak_ns: u64,
    pub ahead_fill: u64,
    pub ahead_aim: u64,
}

/// The last interval read, for `SESSION_AUDIO`.
static LAST: Mutex<Option<Health>> = Mutex::new(None);

/// The last interval read, `None` before the device has played.
#[must_use]
pub fn last() -> Option<Health> {
    LAST.lock().ok().and_then(|l| l.clone())
}

/// Every gap since the engine started, µs ([`Health::gap_ms`]).
static GAP_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The time the device has gone without audio since the engine started,
/// ms.
#[must_use]
pub fn gap_total_ms() -> f64 {
    GAP_US.load(Relaxed) as f64 / 1e3
}

/// The worst render `daw` profiled since the last call, its tracks named
/// from the current song.
fn worst_block(daw: &daw::standalone::Standalone) -> Option<WorstBlock> {
    let p = daw.take_block_profile()?;
    let song = crate::open::current_song();
    let name = |i: u32| {
        song.as_deref()
            .and_then(|g| daw.read_project(g, |s| s.tracks.get(i as usize).map(|t| t.name.clone())))
            .flatten()
            .unwrap_or_else(|| format!("track {i}"))
    };
    Some(WorstBlock::of(&p, name))
}

/// Read `stats` every two seconds, log each interval, keep the last.
pub fn watch(daw: daw::standalone::Standalone, stats: std::sync::Arc<EngineStats>, rate: u32) {
    let spawned = std::thread::Builder::new()
        .name("session-engine-audio-health".into())
        .spawn(move || {
            let mut before = Reading::of(&stats);
            let mut began = std::time::SystemTime::now();
            let mut clock = std::time::Instant::now();
            let mut lag = Lag::default();
            let mut drops_seen = 0u64;
            let (mut last_line, mut since_line) = (None, 0u32);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let now = Reading::of(&stats);
                let elapsed_ms = clock.elapsed().as_secs_f64() * 1e3;
                clock = std::time::Instant::now();
                let ended = std::time::SystemTime::now();
                let at_end = AtEnd {
                    frames: stats.block_frames.load(Relaxed),
                    peak_ns: stats.peak_render_ns.swap(0, Relaxed),
                    ahead_fill: stats.ahead_fill_frames.load(Relaxed),
                    ahead_aim: stats.ahead_depth_frames.load(Relaxed),
                };
                let memory = audio_memory();
                record(memory);
                let mut health = Health {
                    memory: SourceMemory::of(memory),
                    ..Health::between(before, now, at_end, rate)
                };
                health.worst = worst_block(&daw);
                let streaming = stats.stream_state.load(Relaxed) == STREAMING;
                if streaming && (health.blocks > 0 || before.calls > 0) {
                    let delivered_ms = ms(now.frames.saturating_sub(before.frames) as f64, rate);
                    health.gap_ms = lag.step(elapsed_ms, delivered_ms, wander_ms(health.budget_ms));
                } else {
                    // Stopped, or not started: nothing is owed the device.
                    lag = Lag::default();
                }
                before = now;
                GAP_US.fetch_add((health.gap_ms * 1e3) as u64, Relaxed);
                let drops = stats.drops.collect(&mut drops_seen);
                let interval = (began, ended);
                began = ended;
                if health.blocks == 0 && health.gap_ms == 0.0 {
                    continue;
                }
                if health.missed() {
                    trace::dropouts(&health, &drops, interval);
                }
                since_line += 1;
                if health.missed() || worth_a_line(last_line, &health, since_line) {
                    log(&health);
                    (last_line, since_line) = (Some(health.gist()), 0);
                }
                metrics::interval(&health);
                if let Ok(mut last) = LAST.lock() {
                    *last = Some(health);
                }
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "audio health logging not started");
    }
}

/// Each interval as OpenTelemetry metrics, through the global meter (a
/// no-op until the app installs a provider), from this reader thread —
/// never from the audio callback:
///
/// - `audio.render.duration` (ms, gauge): the block's render, `audio.stat`
///   = `mean` | `max` over the interval;
/// - `audio.block.budget` (ms) and `audio.block.size` (frames), gauges;
/// - `audio.load` (1, gauge): render over budget, `mean` | `max`;
/// - `audio.blocks`, `audio.overruns` (blocks over budget), `audio.xruns`,
///   counters; `audio.gap` (ms the device went without audio, [`Lag`]), a
///   counter; `audio.smooth` (1: nothing missed in the interval, 0: a
///   dropout), a gauge;
/// - the render ahead: `audio.render_ahead.depth` and `.aim` (ms) and
///   `.load` (1), gauges; `.short` (blocks that found too little),
///   `.restarts`, `.rewinds`, counters, and `.stale` (ms played from
///   before an edit), a counter.
pub mod metrics {
    use std::sync::OnceLock;

    use opentelemetry::KeyValue;
    use opentelemetry::metrics::{Counter, Gauge, Meter};

    use super::Health;

    /// The instruments, made from one meter.
    pub struct Instruments {
        render: Gauge<f64>,
        budget: Gauge<f64>,
        size: Gauge<u64>,
        load: Gauge<f64>,
        blocks: Counter<u64>,
        overruns: Counter<u64>,
        xruns: Counter<u64>,
        gap: Counter<f64>,
        smooth: Gauge<u64>,
        worst: Gauge<f64>,
        ahead_depth: Gauge<f64>,
        ahead_aim: Gauge<f64>,
        ahead_load: Gauge<f64>,
        ahead_short: Counter<u64>,
        ahead_restarts: Counter<u64>,
        ahead_rewinds: Counter<u64>,
        ahead_stale: Counter<f64>,
    }

    impl Instruments {
        #[must_use]
        pub fn new(meter: &Meter) -> Self {
            let gauge = |name: &'static str, unit: &'static str, about: &'static str| {
                meter
                    .f64_gauge(name)
                    .with_unit(unit)
                    .with_description(about)
                    .build()
            };
            let counter = |name: &'static str, unit: &'static str, about: &'static str| {
                meter
                    .u64_counter(name)
                    .with_unit(unit)
                    .with_description(about)
                    .build()
            };
            Self {
                render: gauge(
                    "audio.render.duration",
                    "ms",
                    "Render time of an audio block, mean and max over the interval",
                ),
                budget: gauge(
                    "audio.block.budget",
                    "ms",
                    "Time an audio block has to render in",
                ),
                size: meter
                    .u64_gauge("audio.block.size")
                    .with_unit("{frame}")
                    .with_description("The device's block")
                    .build(),
                load: gauge("audio.load", "1", "Render time over the block's budget"),
                blocks: counter("audio.blocks", "{block}", "Audio blocks rendered"),
                overruns: counter(
                    "audio.overruns",
                    "{block}",
                    "Blocks that took longer than their budget",
                ),
                xruns: counter(
                    "audio.xruns",
                    "{xrun}",
                    "Blocks the device reported as xruns",
                ),
                gap: meter
                    .f64_counter("audio.gap")
                    .with_unit("ms")
                    .with_description("Time the device went without audio from the engine")
                    .build(),
                smooth: meter
                    .u64_gauge("audio.smooth")
                    .with_unit("1")
                    .with_description(
                        "1 when every block of the interval made its deadline and the device \
                         never went without audio; 0 when not",
                    )
                    .build(),
                worst: gauge(
                    "audio.worst_block.duration",
                    "ms",
                    "The interval's slowest render (a block or a chunk ahead) by `audio.stage`: \
                     total, snapshot, plugin_lock, items, fx",
                ),
                ahead_depth: gauge(
                    "audio.render_ahead.depth",
                    "ms",
                    "Audio rendered ahead of the playhead",
                ),
                ahead_aim: gauge(
                    "audio.render_ahead.aim",
                    "ms",
                    "How far ahead the render ahead aims to be",
                ),
                ahead_load: gauge(
                    "audio.render_ahead.load",
                    "1",
                    "The render ahead's time over the audio it rendered",
                ),
                ahead_short: counter(
                    "audio.render_ahead.short",
                    "{block}",
                    "Blocks that found less rendered ahead than they needed",
                ),
                ahead_restarts: counter(
                    "audio.render_ahead.restarts",
                    "{restart}",
                    "Times the render ahead started again",
                ),
                ahead_rewinds: counter(
                    "audio.render_ahead.rewinds",
                    "{rewind}",
                    "Edits that took the render ahead back to the playhead",
                ),
                ahead_stale: meter
                    .f64_counter("audio.render_ahead.stale")
                    .with_unit("ms")
                    .with_description("Time played from before an edit while its audio came")
                    .build(),
            }
        }

        /// Record one interval.
        pub fn record(&self, h: &Health) {
            let mean = [KeyValue::new("audio.stat", "mean")];
            let max = [KeyValue::new("audio.stat", "max")];
            self.render.record(h.mean_ms, &mean);
            self.render.record(h.peak_ms, &max);
            self.budget.record(h.budget_ms, &[]);
            self.size.record(u64::from(h.frames), &[]);
            if h.budget_ms > 0.0 {
                self.load.record(h.mean_ms / h.budget_ms, &mean);
                self.load.record(h.peak_ms / h.budget_ms, &max);
            }
            self.blocks.add(h.blocks, &[]);
            self.overruns.add(h.over_budget, &[]);
            self.xruns.add(h.xruns, &[]);
            self.gap.add(h.gap_ms, &[]);
            self.smooth.record(u64::from(!h.missed()), &[]);
            if let Some(w) = &h.worst {
                for (stage, v) in [
                    ("total", w.total_ms),
                    ("snapshot", w.snapshot_ms),
                    ("plugin_lock", w.plugin_lock_ms),
                    ("items", w.items_ms),
                    ("fx", w.fx_ms),
                ] {
                    self.worst.record(v, &[KeyValue::new("audio.stage", stage)]);
                }
            }
            self.ahead_depth.record(h.ahead_ms, &[]);
            self.ahead_aim.record(h.ahead_aim_ms, &[]);
            self.ahead_load.record(h.ahead_load, &[]);
            self.ahead_short.add(h.ahead_underruns, &[]);
            self.ahead_restarts.add(h.ahead_restarts, &[]);
            self.ahead_rewinds.add(h.ahead_rewinds, &[]);
            self.ahead_stale.add(h.ahead_stale_ms, &[]);
        }
    }

    /// Record one interval on the global meter's instruments, made on the
    /// first interval from the provider installed then (the app installs
    /// its telemetry before it opens the device).
    pub fn interval(h: &Health) {
        static MADE: OnceLock<Instruments> = OnceLock::new();
        MADE.get_or_init(|| Instruments::new(&opentelemetry::global::meter("session-engine")))
            .record(h);
    }
}

/// An interval that missed, as a trace: `audio.dropouts` over the interval,
/// with each dropped block an `audio.dropout` span when it happened — so it
/// sits in the trace view beside what the engine was doing then.
mod trace {
    use std::time::{Duration, SystemTime};

    use daw::standalone::audio_engine::{DropEvent, DropKind, clock_ns};
    use opentelemetry::KeyValue;
    use opentelemetry::trace::{Span as _, TraceContextExt as _, Tracer as _};

    use super::Health;

    pub fn dropouts(h: &Health, drops: &[DropEvent], (began, ended): (SystemTime, SystemTime)) {
        let tracer = opentelemetry::global::tracer("session-engine");
        let worst_ms = drops.iter().map(|d| d.render_ns).max().unwrap_or(0) as f64 / 1e6;
        let mut root = tracer
            .span_builder("audio.dropouts")
            .with_start_time(began)
            .with_attributes(vec![
                KeyValue::new(
                    "audio.dropouts",
                    i64::try_from(drops.len()).unwrap_or(i64::MAX),
                ),
                KeyValue::new("audio.gap_ms", h.gap_ms),
                KeyValue::new("audio.worst_ms", worst_ms),
                KeyValue::new("audio.budget_ms", h.budget_ms),
                KeyValue::new(
                    "audio.overruns",
                    i64::try_from(h.over_budget).unwrap_or(i64::MAX),
                ),
                KeyValue::new("audio.xruns", i64::try_from(h.xruns).unwrap_or(i64::MAX)),
                KeyValue::new(
                    "audio.render_ahead.short",
                    i64::try_from(h.ahead_underruns).unwrap_or(i64::MAX),
                ),
                KeyValue::new("audio.blocks", i64::try_from(h.blocks).unwrap_or(i64::MAX)),
                KeyValue::new("audio.render_ahead.load", h.ahead_load),
            ])
            .start(&tracer);
        if let Some(w) = &h.worst {
            root.set_attributes([
                KeyValue::new("audio.worst.frames", i64::from(w.frames)),
                KeyValue::new("audio.worst.total_ms", w.total_ms),
                KeyValue::new("audio.worst.snapshot_ms", w.snapshot_ms),
                KeyValue::new("audio.worst.plugin_lock_ms", w.plugin_lock_ms),
                KeyValue::new("audio.worst.items_ms", w.items_ms),
                KeyValue::new("audio.worst.fx_ms", w.fx_ms),
                KeyValue::new("audio.worst.items_track", w.slowest_items_track.clone()),
                KeyValue::new("audio.worst.items_track_ms", w.slowest_items_ms),
                KeyValue::new("audio.worst.fx_track", w.slowest_fx_track.clone()),
                KeyValue::new("audio.worst.fx_track_ms", w.slowest_fx_ms),
            ]);
        }
        let cx = opentelemetry::Context::current_with_span(root);
        // The drops' clock against the wall clock, read together.
        let (clock_now, wall_now) = (clock_ns(), SystemTime::now());
        let wall = |at_ns: u64| {
            wall_now
                .checked_sub(Duration::from_nanos(clock_now.saturating_sub(at_ns)))
                .unwrap_or(began)
        };
        for d in drops {
            // A drop is pushed when its block ends.
            let end = wall(d.at_ns);
            let start = end
                .checked_sub(Duration::from_nanos(d.render_ns))
                .unwrap_or(end);
            let mut span = tracer
                .span_builder("audio.dropout")
                .with_start_time(start)
                .with_attributes(vec![
                    KeyValue::new(
                        "audio.kind",
                        match d.kind {
                            DropKind::OverBudget => "over_budget",
                            DropKind::DeviceOverload => "device_overload",
                        },
                    ),
                    KeyValue::new("audio.render_ms", d.render_ns as f64 / 1e6),
                    KeyValue::new("audio.budget_ms", d.budget_ns as f64 / 1e6),
                    KeyValue::new("audio.frames", i64::from(d.frames)),
                ])
                .start_with_context(&tracer, &cx);
            span.end_with_timestamp(end);
        }
        cx.span().end_with_timestamp(ended);
    }
}

fn log(h: &Health) {
    let Health {
        blocks,
        frames,
        budget_ms,
        mean_ms,
        peak_ms,
        over_budget,
        xruns,
        gap_ms,
        ahead_ms,
        ahead_aim_ms,
        ahead_underruns,
        ahead_restarts,
        ahead_rewinds,
        ahead_stale_ms,
        ahead_load,
        worst,
        memory,
    } = h.clone();
    if h.missed() {
        let worst = worst.unwrap_or_default();
        tracing::warn!(
            target: "session_engine::audio",
            blocks, frames, budget_ms, mean_ms, peak_ms, over_budget, xruns, gap_ms,
            ahead_ms, ahead_aim_ms, ahead_underruns, ahead_restarts, ahead_rewinds,
            ahead_stale_ms, ahead_load,
            worst.total_ms, worst.items_ms, worst.fx_ms,
            worst.items_track = %worst.slowest_items_track,
            worst.items_track_ms = worst.slowest_items_ms,
            "audio: blocks missed their deadline"
        );
    } else {
        tracing::info!(
            target: "session_engine::audio",
            blocks, frames, budget_ms, mean_ms, peak_ms,
            ahead_ms, ahead_aim_ms, ahead_restarts, ahead_rewinds, ahead_stale_ms, ahead_load,
            sources = memory.sources, mapped_mb = memory.mapped_mb,
            streamed_mb = memory.streamed_mb, resident_mb = memory.resident_mb,
            "audio: keeping up"
        );
    }
}

#[cfg(feature = "native")]
mod action {
    use architect::action::{ActionList, ActionMeta, Effect};

    static AUDIO: ActionMeta = crate::actions::meta(
        "SESSION_AUDIO",
        "session_audio",
        "Audio health",
        "How the audio callback kept up over the last two seconds: the device's block (`frames`) and its `budget_ms`, the render's `mean_ms` and `peak_ms`, blocks `over_budget` and `xruns`, and `gap_ms` (time the device went without audio from the engine: a dropout, whatever the render timing says — a callback that blocked or never ran counts here); and the render ahead (the tracks nobody plays live rendered ahead of the playhead, the rest in the callback): `ahead_ms` rendered ahead now and `ahead_aim_ms` how far it aims (its depth, less while it keeps up with time to spare or while edits come in), `ahead_underruns` (blocks that played the tracks ahead short), `ahead_restarts` (a locate, play, a changed live set), `ahead_rewinds` (edits to tracks ahead, each heard from just past the playhead) and `ahead_stale_ms` (time played from before an edit while its frames came: the edit heard that late), and `ahead_load` (its render time over realtime: past 1 it cannot keep up); and `memory`, what the audio sources hold: `sources` (one per recording), `mapped_files` and `mapped_mb` (paged in from disk around the playhead), `streamed_mb` (proxies' chunks) and `resident_mb` (media decoded whole). Null before the device has played. Read-only.",
        "View",
        "",
    );

    pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
        list.register(&AUDIO, Effect::ReadOnly, |(): ()| {
            Ok(super::LAST.lock().ok().and_then(|l| l.clone()))
        })?;
        Ok(())
    }
}

#[cfg(feature = "native")]
pub use action::register;

#[cfg(test)]
mod tests;
