//! A frame that missed 120 Hz, as a trace: what the paint thread did in it.
//!
//! The frame metrics say how often frames are late; this says why. Every
//! span that opens and closes on the paint thread is noted ([`WorkLayer`]):
//! its name, its parent, when it ran. When a paint comes too long after the
//! one before ([`painted`]), the gap is exported as `ui.frame.slow`, with
//! the frame's costs on it and the work done in the gap nested under it at
//! the times it ran — `window.resync` taking 24 ms of a 31 ms frame reads as
//! exactly that. A frame on time drops what was noted: nothing is exported.
//!
//! The spans go through OpenTelemetry's global tracer (a no-op until the app
//! installs a provider), not `tracing`: a frame's span starts in the past,
//! at the paint before.

use std::cell::Cell;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use opentelemetry::trace::{TraceContextExt as _, Tracer as _};
use opentelemetry::{Context, KeyValue};
use tracing::span::{Attributes, Id};
use tracing_subscriber::layer::Context as LayerContext;
use tracing_subscriber::registry::LookupSpan;

use super::metrics::{MISSED_AT, budget_ms};
use super::{AT_REST, Frame, ms};

/// Work shorter than this is not worth a span of its own, microseconds.
const WORTH: u128 = 200;

/// How much work a frame keeps at most: a frame past it is late enough
/// that the first of it says why.
const KEPT: usize = 512;

thread_local! {
    /// Whether this thread paints the window (set by [`painted`]).
    static PAINTS: Cell<bool> = const { Cell::new(false) };
}

/// One span the paint thread ran.
#[derive(Clone, Debug)]
struct Work {
    id: u64,
    parent: Option<u64>,
    name: &'static str,
    start: SystemTime,
    end: SystemTime,
}

/// What the paint thread did since its last paint, in the order it ended.
static WORK: Mutex<Vec<Work>> = Mutex::new(Vec::new());

/// When a span on the paint thread began.
struct Began(SystemTime);

/// The layer that notes the paint thread's work for [`painted`]. Off the
/// paint thread it costs a thread-local read.
pub struct WorkLayer;

impl<S> tracing_subscriber::Layer<S> for WorkLayer
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, _attrs: &Attributes<'_>, id: &Id, ctx: LayerContext<'_, S>) {
        if !PAINTS.with(Cell::get) {
            return;
        }
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(Began(SystemTime::now()));
        }
    }

    fn on_close(&self, id: Id, ctx: LayerContext<'_, S>) {
        if !PAINTS.with(Cell::get) {
            return;
        }
        let Some(span) = ctx.span(&id) else {
            return;
        };
        let Some(start) = span.extensions().get::<Began>().map(|b| b.0) else {
            return;
        };
        let end = SystemTime::now();
        if end.duration_since(start).unwrap_or_default().as_micros() < WORTH {
            return;
        }
        let work = Work {
            id: id.into_u64(),
            parent: span.parent().map(|p| p.id().into_u64()),
            name: span.metadata().name(),
            start,
            end,
        };
        let mut kept = WORK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if kept.len() < KEPT {
            kept.push(work);
        }
    }
}

/// A paint, from the paint thread: when it came too long after the one
/// before, the gap exported as `ui.frame.slow` with what ran in it; the
/// work noted so far let go either way.
pub fn painted(frame: &Frame) {
    PAINTS.with(|p| p.set(true));
    let work = std::mem::take(
        &mut *WORK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    let gap = ms(frame.since);
    let late = frame.since > 0 && frame.since < AT_REST && gap > budget_ms(120) * MISSED_AT;
    if late {
        export(frame, &work);
    }
}

/// `frame`, `frame.since` before now to now, with `work` (what ran in the
/// gap) nested under it.
fn export(frame: &Frame, work: &[Work]) {
    let tracer = opentelemetry::global::tracer("session-ui");
    let end = SystemTime::now();
    let start = end
        .checked_sub(Duration::from_micros(frame.since))
        .unwrap_or(end);
    let mut work: Vec<&Work> = work.iter().filter(|w| w.end >= start).collect();
    work.sort_by_key(|w| w.start);
    // What took the frame: the longest work with no parent in the gap.
    let top = |w: &&&Work| w.parent.is_none_or(|p| !work.iter().any(|o| o.id == p));
    let cause = work
        .iter()
        .filter(top)
        .max_by_key(|w| w.end.duration_since(w.start).unwrap_or_default())
        .map_or("", |w| w.name);
    let work_ms: f64 = work
        .iter()
        .filter(top)
        .map(|w| {
            w.end
                .duration_since(w.start)
                .unwrap_or_default()
                .as_secs_f64()
                * 1e3
        })
        .sum();
    let root = tracer
        .span_builder("ui.frame.slow")
        .with_start_time(start)
        .with_attributes(vec![
            KeyValue::new("ui.frame.interval_ms", ms(frame.since)),
            KeyValue::new("ui.frame.paint_ms", ms(frame.paint)),
            KeyValue::new("ui.frame.prep_ms", ms(frame.prep)),
            KeyValue::new("ui.frame.lanes_ms", ms(frame.lanes)),
            KeyValue::new("ui.frame.titles_ms", ms(frame.titles)),
            KeyValue::new("ui.frame.panel_ms", ms(frame.panel)),
            KeyValue::new("ui.frame.ruler_ms", ms(frame.ruler)),
            KeyValue::new("ui.frame.controls_ms", ms(frame.controls)),
            KeyValue::new("ui.frame.shell_ms", ms(frame.shell)),
            KeyValue::new("ui.frame.encode_ms", ms(frame.encode)),
            KeyValue::new("ui.frame.present_ms", ms(frame.present)),
            KeyValue::new(
                "ui.frame.rebuilds",
                i64::try_from(frame.rebuilds).unwrap_or(i64::MAX),
            ),
            KeyValue::new("ui.frame.work_ms", work_ms),
            KeyValue::new("ui.frame.cause", cause),
        ])
        .start(&tracer);
    let frame_cx = Context::current_with_span(root);
    // Each piece of work under its parent when the parent ran in the gap
    // too, else under the frame: started in order, a parent comes first.
    let mut placed: Vec<(u64, Context, SystemTime)> = Vec::with_capacity(work.len());
    for w in &work {
        let parent = w
            .parent
            .and_then(|p| placed.iter().find(|(id, _, _)| *id == p))
            .map_or_else(|| frame_cx.clone(), |(_, cx, _)| cx.clone());
        let span = tracer
            .span_builder(w.name)
            .with_start_time(w.start)
            .start_with_context(&tracer, &parent);
        placed.push((w.id, parent.with_span(span), w.end));
    }
    for (_, cx, ended) in placed.into_iter().rev() {
        cx.span().end_with_timestamp(ended);
    }
    frame_cx.span().end_with_timestamp(end);
}
