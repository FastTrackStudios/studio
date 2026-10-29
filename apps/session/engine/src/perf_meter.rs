//! The performance meter: REAPER's (View → Performance Meter), read twice
//! a second. The whole engine — its CPU (of all cores), the audio
//! callback's (RT CPU) and the render ahead's, the longest block against
//! its budget, the dropouts so far, disk read and write, its RAM and the
//! system's free — and every track: its FX CPU and media CPU (each of one
//! core: past 100 a track alone cannot keep up), how many FX it has, and
//! their delay compensation (PDC).
//!
//! Each frame goes to every window (`EngineView::perf`), to `SESSION_PERF`,
//! and out as metrics (`process.*`, `system.memory.available`,
//! `audio.rt_cpu`, `audio.track.fx_cpu` / `.media_cpu` by `track.name`).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;
use std::time::{Duration, Instant};

use daw::standalone::Standalone;
use daw::standalone::audio_engine::EngineStats;
use daw::standalone::audio_engine::perf::TrackCost;

use crate::view::{PerfFrame, TrackPerf};

/// How often the meter reads.
const EVERY: Duration = Duration::from_millis(500);

/// How long a plugin's reported latency is trusted before it is asked
/// again: a plugin changes it rarely (a lookahead switched on), and every
/// ask takes the plugin from under the render for a moment.
const PDC_FOR: Duration = Duration::from_secs(30);

/// A track as the meter names it: from the song, read each frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrackInfo {
    pub guid: String,
    pub name: String,
    /// Its FX, in chain order.
    pub fx: Vec<String>,
}

/// Each track's row over an interval of `elapsed_us`: its media and FX
/// time between the two reads as a share of one core (100 = one core the
/// whole time), its FX counted, their latencies (`pdc`, by FX guid) summed.
#[must_use]
pub fn rows(
    before: &[TrackCost],
    now: &[TrackCost],
    elapsed_us: f64,
    tracks: &[TrackInfo],
    pdc: &HashMap<String, u32>,
) -> Vec<TrackPerf> {
    let pct = |us: u64| {
        if elapsed_us > 0.0 {
            us as f64 / elapsed_us * 100.0
        } else {
            0.0
        }
    };
    tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let was = before.get(i).copied().unwrap_or_default();
            let is = now.get(i).copied().unwrap_or_default();
            TrackPerf {
                index: u32::try_from(i).unwrap_or(u32::MAX),
                guid: t.guid.clone(),
                name: t.name.clone(),
                fx_cpu_pct: pct(is.fx_us.saturating_sub(was.fx_us)),
                media_cpu_pct: pct(is.media_us.saturating_sub(was.media_us)),
                fx_count: u32::try_from(t.fx.len()).unwrap_or(u32::MAX),
                pdc_samples: t.fx.iter().filter_map(|g| pdc.get(g)).sum(),
            }
        })
        .collect()
}

/// The process's CPU time so far, seconds, from `/proc/self/stat` (user +
/// system, in the kernel's 100 Hz ticks).
#[must_use]
pub fn cpu_seconds(stat: &str) -> Option<f64> {
    // The name (field 2) is in parentheses and may hold spaces.
    let after = stat.rsplit_once(')')?.1;
    let mut fields = after.split_whitespace();
    // utime and stime are fields 14 and 15: the 12th and 13th after ')'.
    let utime: f64 = fields.nth(11)?.parse().ok()?;
    let stime: f64 = fields.next()?.parse().ok()?;
    Some((utime + stime) / 100.0)
}

/// `key`'s value in a `key: value [kB]` file (`/proc/meminfo`,
/// `/proc/self/status`, `/proc/self/io`), bytes when it says kB.
#[must_use]
pub fn field(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        if k.trim() != key {
            return None;
        }
        let mut parts = v.split_whitespace();
        let n: u64 = parts.next()?.parse().ok()?;
        Some(if parts.next() == Some("kB") {
            n * 1024
        } else {
            n
        })
    })
}

/// What the system says at one moment.
#[derive(Clone, Copy, Debug, Default)]
struct System {
    cpu_s: f64,
    read: u64,
    written: u64,
    ram: u64,
    free: u64,
    total: u64,
}

impl System {
    fn now() -> Self {
        let read = |p: &str| std::fs::read_to_string(p).unwrap_or_default();
        let (stat, io, status, meminfo) = (
            read("/proc/self/stat"),
            read("/proc/self/io"),
            read("/proc/self/status"),
            read("/proc/meminfo"),
        );
        Self {
            cpu_s: cpu_seconds(&stat).unwrap_or(0.0),
            read: field(&io, "read_bytes").unwrap_or(0),
            written: field(&io, "write_bytes").unwrap_or(0),
            ram: field(&status, "VmRSS").unwrap_or(0),
            free: field(&meminfo, "MemAvailable").unwrap_or(0),
            total: field(&meminfo, "MemTotal").unwrap_or(0),
        }
    }
}

/// The current song's tracks, named, with their FX.
fn tracks_of(daw: &Standalone, song: &str) -> Vec<TrackInfo> {
    use daw::standalone::sync::FxChainKey;
    daw.read_project(song, |p| {
        p.tracks
            .iter()
            .map(|t| TrackInfo {
                guid: t.guid.clone(),
                name: t.name.clone(),
                fx: p
                    .fx_chains
                    .get(&FxChainKey::Track(t.guid.clone()))
                    .map(|chain| chain.iter().map(|e| e.fx.guid.clone()).collect())
                    .unwrap_or_default(),
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Each plugin's latency, asked when it is new to the meter and again
/// after [`PDC_FOR`].
#[derive(Default)]
struct Pdc {
    known: HashMap<String, (u32, Instant)>,
}

impl Pdc {
    fn update(&mut self, daw: &Standalone, tracks: &[TrackInfo]) -> HashMap<String, u32> {
        for guid in tracks.iter().flat_map(|t| &t.fx) {
            let stale = self
                .known
                .get(guid)
                .is_none_or(|(_, at)| at.elapsed() >= PDC_FOR);
            if stale && let Some(samples) = daw.fx_latency_now(guid) {
                self.known.insert(guid.clone(), (samples, Instant::now()));
            }
        }
        self.known
            .iter()
            .map(|(g, (s, _))| (g.clone(), *s))
            .collect()
    }
}

/// Read the meter every [`EVERY`] on a thread of its own, from `stats` (the
/// device's engine) and `daw`, and publish each frame.
pub fn watch(daw: Standalone, stats: Arc<EngineStats>, _rate: u32) {
    let spawned = std::thread::Builder::new()
        .name("session-engine-perf-meter".into())
        .spawn(move || {
            let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
            let mut clock = Instant::now();
            let mut sys = System::now();
            let mut render = (
                stats.total_render_ns.load(Relaxed),
                stats.ahead_render_ns.load(Relaxed),
            );
            let mut costs: Vec<TrackCost> = Vec::new();
            let mut pdc = Pdc::default();
            let mut version = 0u64;
            loop {
                std::thread::sleep(EVERY);
                let elapsed = clock.elapsed();
                clock = Instant::now();
                let elapsed_us = elapsed.as_secs_f64() * 1e6;
                let song = crate::open::current_song().unwrap_or_default();
                let tracks = if song.is_empty() {
                    Vec::new()
                } else {
                    tracks_of(&daw, &song)
                };
                let now_costs = daw.track_costs(tracks.len());
                // A song changed under the meter: its tracks are not the
                // ones counted before.
                if now_costs.len() != costs.len() {
                    costs.clone_from(&now_costs);
                }
                let latencies = pdc.update(&daw, &tracks);
                let rows = rows(&costs, &now_costs, elapsed_us, &tracks, &latencies);
                costs = now_costs;

                let now_sys = System::now();
                let secs = elapsed.as_secs_f64().max(1e-9);
                let now_render = (
                    stats.total_render_ns.load(Relaxed),
                    stats.ahead_render_ns.load(Relaxed),
                );
                let core_pct = |ns: u64| ns as f64 / 1e9 / secs * 100.0;
                let health = crate::audio_health::last();
                version += 1;
                let frame = PerfFrame {
                    version,
                    song,
                    cores: u32::try_from(cores).unwrap_or(u32::MAX),
                    cpu_pct: (now_sys.cpu_s - sys.cpu_s).max(0.0) / secs / cores as f64 * 100.0,
                    rt_cpu_pct: core_pct(now_render.0.saturating_sub(render.0)),
                    ahead_cpu_pct: core_pct(now_render.1.saturating_sub(render.1)),
                    rt_longest_ms: health.as_ref().map_or(0.0, |h| h.peak_ms),
                    block_ms: health.as_ref().map_or(0.0, |h| h.budget_ms),
                    overruns: stats.over_budget.load(Relaxed),
                    xruns: stats.xruns.load(Relaxed),
                    gap_ms: crate::audio_health::gap_total_ms(),
                    ahead_short: stats.ahead_underruns.load(Relaxed),
                    disk_read_bps: now_sys.read.saturating_sub(sys.read) as f64 / secs,
                    disk_write_bps: now_sys.written.saturating_sub(sys.written) as f64 / secs,
                    ram_bytes: now_sys.ram,
                    system_free_bytes: now_sys.free,
                    system_total_bytes: now_sys.total,
                    tracks: rows,
                };
                sys = now_sys;
                render = now_render;
                metrics::record(&frame);
                crate::studio_engine::view_host::perf(frame);
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "performance meter not started");
    }
}

/// Each frame as metrics, through the global meter (a no-op until the app
/// installs a provider).
mod metrics {
    use std::sync::OnceLock;

    use opentelemetry::KeyValue;
    use opentelemetry::metrics::Gauge;

    use crate::view::PerfFrame;

    struct Instruments {
        cpu: Gauge<f64>,
        rt_cpu: Gauge<f64>,
        disk: Gauge<f64>,
        ram: Gauge<u64>,
        free: Gauge<u64>,
        track_fx: Gauge<f64>,
        track_media: Gauge<f64>,
        track_pdc: Gauge<u64>,
    }

    /// A track under this share of a core (both columns) is left out of
    /// the metrics: a hundred idle tracks are a hundred series of zeros.
    const QUIET_PCT: f64 = 0.05;

    pub fn record(f: &PerfFrame) {
        static MADE: OnceLock<Instruments> = OnceLock::new();
        let at = MADE.get_or_init(|| {
            let m = opentelemetry::global::meter("session-engine");
            let pct = |name: &'static str, about: &'static str| {
                m.f64_gauge(name)
                    .with_unit("%")
                    .with_description(about)
                    .build()
            };
            Instruments {
                cpu: pct("process.cpu.percent", "The engine's CPU, of all cores"),
                rt_cpu: pct(
                    "audio.rt_cpu",
                    "Render time of one core, by `audio.thread`: callback (RT) or ahead",
                ),
                disk: m
                    .f64_gauge("process.disk.rate")
                    .with_unit("By/s")
                    .with_description("The engine's disk I/O, by `disk.io.direction`")
                    .build(),
                ram: m
                    .u64_gauge("process.memory.resident")
                    .with_unit("By")
                    .with_description("The engine's resident memory")
                    .build(),
                free: m
                    .u64_gauge("system.memory.available")
                    .with_unit("By")
                    .with_description("Memory the system has available")
                    .build(),
                track_fx: pct(
                    "audio.track.fx_cpu",
                    "A track's FX, of one core, by `track.name`",
                ),
                track_media: pct(
                    "audio.track.media_cpu",
                    "A track's items (read, resampled, stretched), of one core, by `track.name`",
                ),
                track_pdc: m
                    .u64_gauge("audio.track.pdc")
                    .with_unit("{sample}")
                    .with_description("A track's FX delay compensation, by `track.name`")
                    .build(),
            }
        });
        at.cpu.record(f.cpu_pct, &[]);
        at.rt_cpu
            .record(f.rt_cpu_pct, &[KeyValue::new("audio.thread", "callback")]);
        at.rt_cpu
            .record(f.ahead_cpu_pct, &[KeyValue::new("audio.thread", "ahead")]);
        at.disk.record(
            f.disk_read_bps,
            &[KeyValue::new("disk.io.direction", "read")],
        );
        at.disk.record(
            f.disk_write_bps,
            &[KeyValue::new("disk.io.direction", "write")],
        );
        at.ram.record(f.ram_bytes, &[]);
        at.free.record(f.system_free_bytes, &[]);
        for t in &f.tracks {
            let busy = t.fx_cpu_pct >= QUIET_PCT || t.media_cpu_pct >= QUIET_PCT;
            if !busy && t.pdc_samples == 0 {
                continue;
            }
            let name = [KeyValue::new("track.name", t.name.clone())];
            at.track_fx.record(t.fx_cpu_pct, &name);
            at.track_media.record(t.media_cpu_pct, &name);
            at.track_pdc.record(u64::from(t.pdc_samples), &name);
        }
    }
}

static PERF: architect::action::ActionMeta = crate::actions::meta(
    "SESSION_PERF",
    "session_perf",
    "Performance meter",
    "The performance meter, as REAPER's: the engine's `cpu_pct` (of all `cores`), the audio callback's `rt_cpu_pct` and the render ahead's `ahead_cpu_pct` (of one core), `rt_longest_ms` (the longest block of the last two seconds) against `block_ms`; since the engine started, `overruns` (blocks over their budget), `xruns`, `gap_ms` (time the device went without audio) and `ahead_short`; `disk_read_bps`/`disk_write_bps`, `ram_bytes`, `system_free_bytes`. And every track of `song`: `fx_cpu_pct` and `media_cpu_pct` (its items read, resampled, stretched; of one core — past 100 the track alone cannot keep up), `fx_count`, `pdc_samples`. `sort` (`fx`, `media`, `cpu`: both; default the song's order) and `limit`. Read twice a second. Read-only.",
    "View",
    "",
);

/// How to give the tracks.
#[derive(facet::Facet)]
struct PerfArg {
    sort: Option<String>,
    limit: Option<u32>,
}

/// `SESSION_PERF`.
///
/// # Errors
/// When the action is already registered.
pub fn register(
    list: &architect::action::ActionList,
) -> Result<(), architect::action::ActionError> {
    list.register(
        &PERF,
        architect::action::Effect::ReadOnly,
        |PerfArg { sort, limit }: PerfArg| {
            let mut frame = crate::studio_engine::view_host::last_perf();
            let key = |t: &TrackPerf| match sort.as_deref() {
                Some("fx") => t.fx_cpu_pct,
                Some("media") => t.media_cpu_pct,
                Some("cpu") => t.fx_cpu_pct + t.media_cpu_pct,
                _ => 0.0,
            };
            if sort.is_some() {
                frame.tracks.sort_by(|a, b| key(b).total_cmp(&key(a)));
            }
            if let Some(n) = limit {
                frame.tracks.truncate(n as usize);
            }
            Ok(frame)
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
