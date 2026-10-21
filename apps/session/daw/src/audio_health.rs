//! How the audio callback is keeping up, read every two seconds: the
//! block the device runs at, render time (mean and worst) against that
//! block's budget, blocks that overran it or that the device called
//! xruns — and the render ahead (the dual-buffer engine): how much is
//! rendered ahead of the playhead and how much it aims for, blocks that
//! found too little, restarts, edits that took it back, frames played
//! from before an edit while the edit's came, and what rendering ahead
//! costs against realtime. Read over MCP with `SESSION_AUDIO`.
//!
//! Logged as `session_daw::audio`: a warning for every interval a block
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
        .name("session-daw-audio-health".into())
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
                if let Ok(mut last) = LAST.lock() {
                    *last = Some(health);
                }
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "audio health logging not started");
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
            target: "session_daw::audio",
            blocks, frames, budget_ms, mean_ms, peak_ms, over_budget, xruns,
            ahead_ms, ahead_aim_ms, ahead_underruns, ahead_restarts, ahead_rewinds,
            ahead_stale_ms, ahead_load,
            "audio: blocks missed their deadline"
        );
    } else {
        tracing::info!(
            target: "session_daw::audio",
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
        "How the audio callback kept up over the last two seconds: the device's block (`frames`) and its `budget_ms`, the render's `mean_ms` and `peak_ms`, blocks `over_budget` and `xruns`; and the render ahead (the tracks nobody plays live rendered ahead of the playhead, the rest in the callback): `ahead_ms` rendered ahead now and `ahead_aim_ms` how far it aims (its depth, less while it keeps up with time to spare or while edits come in), `ahead_underruns` (blocks the render ahead fell behind on, rendered whole in the callback instead), `ahead_restarts` (a locate, play, a changed live set), `ahead_rewinds` (edits to tracks ahead, each heard from just past the playhead) and `ahead_stale_ms` (time played from before an edit while its frames came: the edit heard that late), and `ahead_load` (its render time over realtime: past 1 it cannot keep up). Null before the device has played. Read-only.",
        "View",
        "",
    );

    pub fn register(list: &ActionList) {
        list.register(&AUDIO, Effect::ReadOnly, |(): ()| {
            Ok(super::LAST.lock().ok().and_then(|l| l.clone()))
        });
    }
}

#[cfg(feature = "native")]
pub use action::register;

#[cfg(test)]
mod tests {
    use super::*;

    /// Two readings two seconds apart: the interval's counts, its mean,
    /// and the render ahead's cost against the audio it made.
    #[test]
    fn an_interval_is_the_difference_of_two_readings() {
        let before = Reading {
            calls: 100,
            render_ns: 100_000_000,
            ahead_frames: 48_000,
            ahead_ns: 1_000_000_000,
            ..Reading::default()
        };
        let now = Reading {
            calls: 850,
            render_ns: 100_000_000 + 750 * 1_000_000,
            over_budget: 2,
            ahead_underruns: 1,
            ahead_frames: 48_000 + 96_000,
            ahead_ns: 1_000_000_000 + 500_000_000,
            ..Reading::default()
        };
        let at_end = AtEnd {
            frames: 128,
            peak_ns: 3_000_000,
            ahead_fill: 9_600,
            ahead_aim: 9_600,
        };
        let h = Health::between(before, now, at_end, 48_000);
        assert_eq!(h.blocks, 750);
        assert!((h.mean_ms - 1.0).abs() < 1e-9);
        assert!((h.budget_ms - 128.0 / 48.0).abs() < 1e-9);
        assert!((h.ahead_ms - 200.0).abs() < 1e-9);
        assert!((h.ahead_load - 0.25).abs() < 1e-9, "half a second for two");
        assert!(h.missed());
    }

    /// A line when how it keeps up changes, or a minute on; not every two
    /// seconds of the same.
    #[test]
    fn keeping_up_is_said_when_it_changes() {
        let health = |aim: f64, load: f64| Health {
            frames: 128,
            ahead_aim_ms: aim,
            ahead_load: load,
            ..Health::default()
        };
        let said = Some(health(200.0, 0.25).gist());
        assert!(
            !worth_a_line(said, &health(200.2, 0.26), 1),
            "the same, near enough"
        );
        assert!(worth_a_line(said, &health(150.0, 0.25), 1), "a new aim");
        assert!(worth_a_line(said, &health(200.0, 0.6), 1), "a new load");
        assert!(worth_a_line(said, &health(200.0, 0.25), 30), "a minute on");
        assert!(worth_a_line(None, &health(200.0, 0.25), 0), "the first");
    }
}
