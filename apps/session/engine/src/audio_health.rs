//! How the audio callback is keeping up, read every two seconds: the
//! block the device runs at, render time (mean and worst) against that
//! block's budget, blocks that overran it or that the device called
//! xruns — and the render ahead (the dual-buffer engine): how much is
//! rendered ahead of the playhead and how much it aims for, blocks that
//! found too little, restarts, edits that took it back, frames played
//! from before an edit while the edit's came, and what rendering ahead
//! costs against realtime. Read over MCP with `SESSION_AUDIO`.
//!
//! Logged as `session_engine::audio`: a warning for every interval a block
//! missed (late, an xrun, or the tracks ahead played short); otherwise
//! one line when how it keeps up changes — the block size, the aim, the
//! load by a tenth — and once a minute while it does not.

use std::sync::Mutex;
use std::sync::atomic::Ordering::Relaxed;

use daw::standalone::audio_engine::EngineStats;
use facet::Facet;

/// The engine's counters at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Reading {
    pub calls: u64,
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
        }
    }

    /// Whether a block was late or played short of the tracks ahead.
    #[must_use]
    pub fn missed(&self) -> bool {
        self.over_budget > 0 || self.xruns > 0 || self.ahead_underruns > 0
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

/// Read `stats` every two seconds, log each interval, keep the last.
pub fn watch(stats: std::sync::Arc<EngineStats>, rate: u32) {
    let spawned = std::thread::Builder::new()
        .name("session-engine-audio-health".into())
        .spawn(move || {
            let mut before = Reading::of(&stats);
            let (mut last_line, mut since_line) = (None, 0u32);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let now = Reading::of(&stats);
                let at_end = AtEnd {
                    frames: stats.block_frames.load(Relaxed),
                    peak_ns: stats.peak_render_ns.swap(0, Relaxed),
                    ahead_fill: stats.ahead_fill_frames.load(Relaxed),
                    ahead_aim: stats.ahead_depth_frames.load(Relaxed),
                };
                let health = Health::between(before, now, at_end, rate);
                before = now;
                if health.blocks == 0 {
                    continue;
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
///   counters;
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

fn log(h: &Health) {
    let Health {
        blocks,
        frames,
        budget_ms,
        mean_ms,
        peak_ms,
        over_budget,
        xruns,
        ahead_ms,
        ahead_aim_ms,
        ahead_underruns,
        ahead_restarts,
        ahead_rewinds,
        ahead_stale_ms,
        ahead_load,
    } = h.clone();
    if h.missed() {
        tracing::warn!(
            target: "session_engine::audio",
            blocks, frames, budget_ms, mean_ms, peak_ms, over_budget, xruns,
            ahead_ms, ahead_aim_ms, ahead_underruns, ahead_restarts, ahead_rewinds,
            ahead_stale_ms, ahead_load,
            "audio: blocks missed their deadline"
        );
    } else {
        tracing::info!(
            target: "session_engine::audio",
            blocks, frames, budget_ms, mean_ms, peak_ms,
            ahead_ms, ahead_aim_ms, ahead_restarts, ahead_rewinds, ahead_stale_ms, ahead_load,
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
        "How the audio callback kept up over the last two seconds: the device's block (`frames`) and its `budget_ms`, the render's `mean_ms` and `peak_ms`, blocks `over_budget` and `xruns`; and the render ahead (the tracks nobody plays live rendered ahead of the playhead, the rest in the callback): `ahead_ms` rendered ahead now and `ahead_aim_ms` how far it aims (its depth, less while it keeps up with time to spare or while edits come in), `ahead_underruns` (blocks that played the tracks ahead short), `ahead_restarts` (a locate, play, a changed live set), `ahead_rewinds` (edits to tracks ahead, each heard from just past the playhead) and `ahead_stale_ms` (time played from before an edit while its frames came: the edit heard that late), and `ahead_load` (its render time over realtime: past 1 it cannot keep up). Null before the device has played. Read-only.",
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
