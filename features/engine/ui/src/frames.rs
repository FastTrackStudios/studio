//! The arrangement's frames, kept so the frame rate can be read at any
//! moment: `SESSION_FRAMES` says how the last seconds went (frames a
//! second, what a frame cost and where it went), waits for fresh frames
//! first if asked, and writes every frame to a CSV file if asked.
//!
//! Each paint records one [`Frame`]: how long since the last one, what
//! the paint cost pass by pass, and what the shell said the frame before
//! cost to encode and present (`blitz_traits::LAST_*_MICROS`). The log is
//! a ring of the last [`KEPT`] frames; reading it costs the window
//! nothing.

use std::collections::VecDeque;
use std::sync::Mutex;

use facet::Facet;

/// How many frames the log keeps: about ten seconds at 240 Hz.
const KEPT: usize = 2_400;

/// A gap longer than this between paints is the window at rest, not a
/// slow frame, microseconds.
const AT_REST: u64 = 200_000;

/// One paint of the arrangement.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frame {
    /// Microseconds since the log began.
    pub at: u64,
    /// Since the paint before, microseconds.
    pub since: u64,
    /// The paint, microseconds, and its passes.
    pub paint: u64,
    pub prep: u64,
    pub lanes: u64,
    pub titles: u64,
    pub panel: u64,
    pub ruler: u64,
    pub controls: u64,
    /// What the shell said the frame before cost, microseconds: all of
    /// it, encoding it, presenting it.
    pub shell: u64,
    pub encode: u64,
    pub present: u64,
    /// Drawing commands replayed from the recording, and submitted.
    pub replayed: u64,
    pub submitted: u64,
    /// Times the arrangement's recording was built again for it: the
    /// whole song re-cut.
    pub rebuilds: u64,
}

impl Frame {
    /// The CSV header, as [`Frame::csv`] writes a row.
    pub const CSV_HEADER: &'static str = "at_us,since_us,paint_us,prep_us,lanes_us,titles_us,panel_us,ruler_us,controls_us,shell_us,encode_us,present_us,replayed,submitted,rebuilds";

    #[must_use]
    pub fn csv(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            self.at,
            self.since,
            self.paint,
            self.prep,
            self.lanes,
            self.titles,
            self.panel,
            self.ruler,
            self.controls,
            self.shell,
            self.encode,
            self.present,
            self.replayed,
            self.submitted,
            self.rebuilds
        )
    }
}

/// How a run of frames went.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct Summary {
    /// Frames in the run.
    pub frames: u32,
    /// Seconds it spans.
    pub seconds: f64,
    /// Frames a second while drawing: one over the median gap between
    /// paints, gaps at rest left out.
    pub fps: f64,
    /// The gap between paints, ms.
    pub gap: Spread,
    /// The paint, ms, and its passes' means.
    pub paint: Spread,
    pub prep_ms: f64,
    pub lanes_ms: f64,
    pub titles_ms: f64,
    pub panel_ms: f64,
    pub ruler_ms: f64,
    pub controls_ms: f64,
    /// The whole frame as the shell timed it, ms, and its encode and
    /// present means.
    pub shell: Spread,
    pub encode_ms: f64,
    pub present_ms: f64,
    /// Drawing commands submitted a frame, on average.
    pub commands: f64,
    /// Times the whole arrangement was built again over the run.
    pub rebuilds: u64,
}

/// A measure over a run: its median, 95th percentile and worst.
#[derive(Clone, Copy, Debug, Default, PartialEq, Facet)]
pub struct Spread {
    pub median: f64,
    pub p95: f64,
    pub max: f64,
}

impl Spread {
    /// Of `micros`, in ms; all zero for none.
    #[must_use]
    pub fn of(micros: impl IntoIterator<Item = u64>) -> Self {
        let mut v: Vec<u64> = micros.into_iter().collect();
        if v.is_empty() {
            return Self::default();
        }
        v.sort_unstable();
        let at = |q: f64| {
            let last = v.len() - 1;
            let i = (q * f64::from(u32::try_from(last).unwrap_or(u32::MAX))).round();
            let i = num_traits::ToPrimitive::to_usize(&i)
                .unwrap_or(last)
                .min(last);
            ms(v[i])
        };
        Self {
            median: at(0.5),
            p95: at(0.95),
            max: at(1.0),
        }
    }
}

fn ms(micros: u64) -> f64 {
    num_traits::ToPrimitive::to_f64(&micros).unwrap_or(0.0) / 1_000.0
}

fn mean(frames: &[Frame], of: impl Fn(&Frame) -> u64) -> f64 {
    if frames.is_empty() {
        return 0.0;
    }
    let sum: u64 = frames.iter().map(of).sum();
    ms(sum) / num_traits::ToPrimitive::to_f64(&frames.len()).unwrap_or(1.0)
}

impl Summary {
    /// How `frames` (oldest first) went.
    #[must_use]
    pub fn of(frames: &[Frame]) -> Self {
        let (Some(first), Some(last)) = (frames.first(), frames.last()) else {
            return Self::default();
        };
        let gaps = || {
            frames
                .iter()
                .skip(1)
                .map(|f| f.since)
                .filter(|s| *s > 0 && *s < AT_REST)
        };
        let gap = Spread::of(gaps());
        let shell = Spread::of(frames.iter().map(|f| f.shell).filter(|s| *s > 0));
        Self {
            frames: u32::try_from(frames.len()).unwrap_or(u32::MAX),
            seconds: ms(last.at.saturating_sub(first.at)) / 1_000.0,
            fps: if gap.median > 0.0 {
                1_000.0 / gap.median
            } else {
                0.0
            },
            gap,
            paint: Spread::of(frames.iter().map(|f| f.paint)),
            prep_ms: mean(frames, |f| f.prep),
            lanes_ms: mean(frames, |f| f.lanes),
            titles_ms: mean(frames, |f| f.titles),
            panel_ms: mean(frames, |f| f.panel),
            ruler_ms: mean(frames, |f| f.ruler),
            controls_ms: mean(frames, |f| f.controls),
            shell,
            encode_ms: mean(frames, |f| f.encode),
            present_ms: mean(frames, |f| f.present),
            commands: num_traits::ToPrimitive::to_f64(
                &frames.iter().map(|f| f.submitted).sum::<u64>(),
            )
            .unwrap_or(0.0)
                / num_traits::ToPrimitive::to_f64(&frames.len()).unwrap_or(1.0),
            rebuilds: frames.iter().map(|f| f.rebuilds).sum(),
        }
    }
}

/// The log: when it began, and the frames since, newest last.
struct Log {
    began: web_time::Instant,
    last: Option<u64>,
    frames: VecDeque<Frame>,
}

static LOG: Mutex<Option<Log>> = Mutex::new(None);

/// Microseconds since the log began (it begins now if it had not).
#[must_use]
pub fn now() -> u64 {
    let mut log = LOG
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let log = log.get_or_insert_with(|| Log {
        began: web_time::Instant::now(),
        last: None,
        frames: VecDeque::with_capacity(KEPT),
    });
    u64::try_from(log.began.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// Keep `frame`, its `at` and `since` filled in here.
pub fn record(mut frame: Frame) {
    let at = now();
    {
        let mut log = LOG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(log) = log.as_mut() else {
            return;
        };
        frame.at = at;
        frame.since = log.last.map_or(0, |l| at.saturating_sub(l));
        log.last = Some(at);
        if log.frames.len() == KEPT {
            log.frames.pop_front();
        }
        log.frames.push_back(frame);
    }
    // Outside the log's lock: the frame rate is observed from it.
    #[cfg(feature = "native")]
    {
        metrics::frame(&frame);
        slow::painted(&frame);
    }
}

/// The frames painted since `from` (microseconds, as [`now`] says),
/// oldest first.
#[must_use]
pub fn since(from: u64) -> Vec<Frame> {
    let log = LOG
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    log.as_ref().map_or_else(Vec::new, |l| {
        l.frames.iter().filter(|f| f.at >= from).copied().collect()
    })
}

/// The frames as OpenTelemetry metrics, through the global meter (a
/// no-op until the app installs a provider):
///
/// - `ui.frame.duration` (ms): what each frame cost, as the shell timed
///   it (resolve, encode, present) or, with no shell, what its paint did;
///   `ui.frame.source` = `shell` | `paint` says which.
/// - `ui.paint.duration` (ms): the arrangement's paint alone.
/// - `ui.frame.over_budget`: frames slower than the 240 Hz budget
///   ([`crate::fps::BUDGET_MS`]).
/// - `ui.frame.interval` (ms): the gap between one paint and the next
///   while drawing — the pacing a viewer sees, whatever kept the window
///   from painting (a paint can be quick and the frame still late).
/// - `ui.frame.missed`: frames that missed their deadline, by
///   `ui.budget_hz` (120, 60): a gap past [`MISSED_AT`] of the budget.
/// - `ui.frames_per_second`: observed when the meter collects, one over
///   the median gap between paints over the last five seconds (as
///   `SESSION_FRAMES` says it), 0 at rest.
/// - `ui.frames_per_second.low`: the same five seconds' "1% low" — one
///   over the slowest 1% of gaps: the stutter the median hides.
///
/// A frame that misses 120 Hz is also a trace (`slow`): `ui.frame.slow`,
/// over the gap, with what the paint thread did in it.
#[cfg(feature = "native")]
pub(crate) mod metrics {
    use std::sync::OnceLock;

    use opentelemetry::KeyValue;
    use opentelemetry::metrics::{Counter, Histogram, Meter};

    use super::Frame;

    /// Bucket bounds, ms: fine around the 240, 120 and 60 Hz budgets
    /// (4.17, 8.33, 16.67), coarse past them.
    const BOUNDS: [f64; 24] = [
        0.5, 1.0, 2.0, 3.0, 3.5, 4.0, 4.17, 5.0, 6.0, 7.0, 8.0, 8.33, 10.0, 12.5, 15.0, 16.67,
        20.0, 25.0, 33.33, 50.0, 75.0, 100.0, 200.0, 500.0,
    ];

    /// How far back the frame rate looks, microseconds.
    const FPS_OVER: u64 = 5_000_000;

    /// The rates a frame's deadline is counted against, Hz.
    pub const DEADLINES: [u32; 2] = [120, 60];

    /// A gap past this much of a deadline's budget missed it: a vsync
    /// interval wanders a little either side of its budget.
    pub const MISSED_AT: f64 = 1.25;

    pub struct Instruments {
        frame: Histogram<f64>,
        paint: Histogram<f64>,
        interval: Histogram<f64>,
        over_budget: Counter<u64>,
        missed: Counter<u64>,
    }

    /// The budget of a frame at `hz`, ms.
    pub fn budget_ms(hz: u32) -> f64 {
        1000.0 / f64::from(hz)
    }

    /// The gaps between paints while drawing (at rest is not a frame),
    /// microseconds.
    fn drawing_gaps(frames: &[Frame]) -> Vec<u64> {
        frames
            .iter()
            .map(|f| f.since)
            .filter(|&gap| gap > 0 && gap < super::AT_REST)
            .collect()
    }

    /// One over the slowest 1% of `gaps` (microseconds), frames a second;
    /// 0 with none.
    pub fn low_fps(mut gaps: Vec<u64>) -> f64 {
        if gaps.is_empty() {
            return 0.0;
        }
        gaps.sort_unstable();
        let at = (gaps.len() * 99 / 100).min(gaps.len() - 1);
        gaps.get(at)
            .map_or(0.0, |&gap| 1000.0 / super::ms(gap).max(f64::EPSILON))
    }

    impl Instruments {
        /// The instruments, on `meter`; the frame rate observed from the
        /// frame log whenever it collects.
        pub fn new(meter: &Meter) -> Self {
            meter
                .f64_observable_gauge("ui.frames_per_second")
                .with_unit("{frame}/s")
                .with_description("Frames a second while drawing, over the last five seconds")
                .with_callback(|observer| {
                    let from = super::now().saturating_sub(FPS_OVER);
                    observer.observe(super::Summary::of(&super::since(from)).fps, &[]);
                })
                .build();
            meter
                .f64_observable_gauge("ui.frames_per_second.low")
                .with_unit("{frame}/s")
                .with_description("The slowest 1% of the last five seconds' frames, as a rate")
                .with_callback(|observer| {
                    let from = super::now().saturating_sub(FPS_OVER);
                    observer.observe(low_fps(drawing_gaps(&super::since(from))), &[]);
                })
                .build();
            Self {
                frame: meter
                    .f64_histogram("ui.frame.duration")
                    .with_unit("ms")
                    .with_description("What each frame of the window cost")
                    .with_boundaries(BOUNDS.to_vec())
                    .build(),
                paint: meter
                    .f64_histogram("ui.paint.duration")
                    .with_unit("ms")
                    .with_description("What the arrangement's paint of each frame cost")
                    .with_boundaries(BOUNDS.to_vec())
                    .build(),
                interval: meter
                    .f64_histogram("ui.frame.interval")
                    .with_unit("ms")
                    .with_description("The gap between one paint and the next while drawing")
                    .with_boundaries(BOUNDS.to_vec())
                    .build(),
                over_budget: meter
                    .u64_counter("ui.frame.over_budget")
                    .with_unit("{frame}")
                    .with_description("Frames slower than the 240 Hz budget")
                    .build(),
                missed: meter
                    .u64_counter("ui.frame.missed")
                    .with_unit("{frame}")
                    .with_description("Frames that missed their deadline, by the rate aimed for")
                    .build(),
            }
        }

        /// Record one paint's frame.
        pub fn record(&self, frame: &Frame) {
            let (source, micros) = if frame.shell > 0 {
                ("shell", frame.shell)
            } else {
                ("paint", frame.paint)
            };
            let ms = super::ms(micros);
            self.frame
                .record(ms, &[KeyValue::new("ui.frame.source", source)]);
            self.paint.record(super::ms(frame.paint), &[]);
            if ms > crate::fps::BUDGET_MS {
                self.over_budget.add(1, &[]);
            }
            // The pacing, while drawing: a gap past rest is the window idle.
            if frame.since > 0 && frame.since < super::AT_REST {
                let gap = super::ms(frame.since);
                self.interval.record(gap, &[]);
                for hz in DEADLINES {
                    if gap > budget_ms(hz) * MISSED_AT {
                        self.missed
                            .add(1, &[KeyValue::new("ui.budget_hz", i64::from(hz))]);
                    }
                }
            }
        }
    }

    /// Record one paint's frame on the global meter's instruments, made on
    /// the first frame from the provider installed then: the app installs
    /// its telemetry before it opens a window.
    pub fn frame(frame: &Frame) {
        static MADE: OnceLock<Instruments> = OnceLock::new();
        MADE.get_or_init(|| Instruments::new(&opentelemetry::global::meter("session-ui")))
            .record(frame);
    }
}

#[cfg(feature = "native")]
pub mod slow;

#[cfg(feature = "native")]
mod action {
    use architect::action::{ActionList, ActionMeta, Effect};
    use facet::Facet;

    use super::{Frame, Summary};

    static FRAMES: ActionMeta = crate::actions::meta(
        "SESSION_FRAMES",
        "session_frames",
        "Frame rate",
        "How the arrangement's frames went: `fps` (one over the median gap between paints while drawing), the gap, the paint and the shell's whole frame as median/p95/max ms, the paint's passes (prep — read-back and re-cuts before drawing —, lanes, titles, panel, ruler, controls) and the shell's encode/present as means, and commands a frame. Over the last `seconds` (default 5); with `wait` (seconds, at most 30) it waits that long first and says only the frames painted meanwhile — play or scroll while it waits. With `file` (a path) it also writes every frame of the run there as CSV. Read-only.",
        "View",
        "",
    );

    #[derive(Facet)]
    struct FramesArg {
        seconds: Option<f64>,
        wait: Option<f64>,
        file: Option<String>,
    }

    #[derive(Facet)]
    struct FramesOut {
        summary: Summary,
        /// Where the CSV went, when asked for.
        file: Option<String>,
    }

    fn micros(seconds: f64) -> u64 {
        num_traits::ToPrimitive::to_u64(&(seconds.max(0.0) * 1e6)).unwrap_or(0)
    }

    pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
        list.register(&FRAMES, Effect::ReadOnly, |a: FramesArg| {
            let from = match a.wait.filter(|w| *w > 0.0) {
                Some(wait) => {
                    let from = super::now();
                    std::thread::sleep(std::time::Duration::from_secs_f64(wait.min(30.0)));
                    from
                }
                None => super::now().saturating_sub(micros(a.seconds.unwrap_or(5.0))),
            };
            let frames = super::since(from);
            if let Some(path) = &a.file {
                let mut csv = String::from(Frame::CSV_HEADER);
                for f in &frames {
                    csv.push('\n');
                    csv.push_str(&f.csv());
                }
                csv.push('\n');
                std::fs::write(path, csv).map_err(|e| format!("{path}: {e}"))?;
            }
            Ok(FramesOut {
                summary: Summary::of(&frames),
                file: a.file,
            })
        })?;
        Ok(())
    }
}

#[cfg(feature = "native")]
pub use action::register;

#[cfg(all(test, feature = "native"))]
mod tests;
