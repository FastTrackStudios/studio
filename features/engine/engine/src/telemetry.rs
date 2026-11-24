//! The engine's telemetry on plain `tracing`: the pieces each process's
//! subscriber is made of (Session's window, `studio engine`, Studio's
//! `daw` extension, the CLI).
//!
//! - [`SpanRing`]: a `tracing_subscriber` layer that keeps the latest
//!   closed units of work (every `action`, every span with timed steps
//!   under it) with their fields, each child span's time and the total, in
//!   a bounded ring: what `PERF_RECENT` reads back ([`recent`]).
//! - [`otel`]: OTLP export of every span (and the global meter's metrics)
//!   when `OTEL_EXPORTER_OTLP_ENDPOINT` (or its `_TRACES_` form) is set.
//! - [`install_panic_hook`]: a panic logged through `tracing`, then the
//!   hook that was there before.
//!
//! A step of a unit of work is a child span, entered and dropped before
//! the next:
//!
//! ```ignore
//! let step = tracing::info_span!("cuts.song").entered();
//! let song = read();
//! drop(step);
//! let step = tracing::info_span!("cuts.slip").entered();
//! ```

use std::collections::VecDeque;
use std::sync::Mutex;
use web_time::{Instant, SystemTime};

use tracing::Subscriber;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

/// The directive that lets the Action List's span through a global
/// `EnvFilter` whatever the log level: every action is timed
/// (`PERF_RECENT`) even under `RUST_LOG=warn`.
pub const DIRECTIVES: [&str; 1] = ["architect::action_list=info"];

/// `filter` with [`DIRECTIVES`] added.
#[must_use]
pub fn with_directives(filter: tracing_subscriber::EnvFilter) -> tracing_subscriber::EnvFilter {
    DIRECTIVES
        .iter()
        .fold(filter, |filter, directive| match directive.parse() {
            Ok(d) => filter.add_directive(d),
            Err(_) => filter,
        })
}

/// Set `key` to `value` on the current span's exported attributes: a
/// field of the wide event known only where it is computed (a span may
/// only record fields it declared; this adds any). Nothing when no span is
/// exported.
pub fn set(key: impl Into<opentelemetry::Key>, value: impl Into<opentelemetry::Value>) {
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;
    tracing::Span::current().set_attribute(key, value);
}

/// One unit of work, as [`SpanRing`] kept it.
#[derive(Clone, Debug, PartialEq)]
pub struct Timed {
    /// The span's name (`action`, `window.slip`).
    pub name: &'static str,
    /// Its fields, as recorded (`action.id = QUANTIZE`).
    pub fields: Vec<(&'static str, String)>,
    /// Its child spans in the order they closed, each its name and ms.
    pub stages: Vec<(&'static str, f64)>,
    /// From opening to closing, ms.
    pub total_ms: f64,
    /// When it closed.
    pub at: SystemTime,
}

impl Timed {
    /// A field's value, if it has it.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }

    /// A step's time, ms (summed when it ran more than once).
    #[must_use]
    pub fn stage(&self, key: &str) -> Option<f64> {
        self.stages
            .iter()
            .filter(|(k, _)| *k == key)
            .map(|(_, ms)| *ms)
            .reduce(|a, b| a + b)
    }
}

/// How many units of work are kept; the oldest go first.
pub const KEPT: usize = 512;

static RECENT: Mutex<VecDeque<Timed>> = Mutex::new(VecDeque::new());

/// The latest `n` units of work kept, newest first.
#[must_use]
pub fn recent(n: usize) -> Vec<Timed> {
    RECENT
        .lock()
        .map(|kept| kept.iter().rev().take(n).cloned().collect())
        .unwrap_or_default()
}

fn keep(timed: Timed) {
    if let Ok(mut kept) = RECENT.lock() {
        while kept.len() >= KEPT {
            kept.pop_front();
        }
        kept.push_back(timed);
    }
}

/// A span open: what [`Timed`] is made of when it closes.
struct Open {
    opened: Instant,
    fields: Vec<(&'static str, String)>,
    stages: Vec<(&'static str, f64)>,
}

/// The layer that keeps closed units of work ([`recent`]): every span
/// that had child spans, and every span named in `always` (an action,
/// whether it has steps or not).
#[derive(Clone, Debug)]
pub struct SpanRing {
    always: &'static [&'static str],
}

impl SpanRing {
    /// Keep the spans with child spans, and every span named one of
    /// `always`.
    #[must_use]
    pub const fn new(always: &'static [&'static str]) -> Self {
        Self { always }
    }
}

impl Default for SpanRing {
    /// Spans with steps, and every Action List action.
    fn default() -> Self {
        Self::new(&["action"])
    }
}

struct Fields<'a>(&'a mut Vec<(&'static str, String)>);

impl Visit for Fields<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.push((field.name(), format!("{value:?}")));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.push((field.name(), value.to_owned()));
    }
}

impl<S> tracing_subscriber::Layer<S> for SpanRing
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        let mut fields = Vec::new();
        attrs.record(&mut Fields(&mut fields));
        span.extensions_mut().insert(Open {
            opened: Instant::now(),
            fields,
            stages: Vec::new(),
        });
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        let mut ext = span.extensions_mut();
        if let Some(open) = ext.get_mut::<Open>() {
            let mut fields = Vec::new();
            values.record(&mut Fields(&mut fields));
            for (k, v) in fields {
                open.fields.retain(|(have, _)| *have != k);
                open.fields.push((k, v));
            }
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(&id) else { return };
        let name = span.name();
        let Some(open) = span.extensions_mut().remove::<Open>() else {
            return;
        };
        let total_ms = open.opened.elapsed().as_secs_f64() * 1e3;
        // A step of its parent: the parent's stage.
        if let Some(parent) = span.parent()
            && let Some(up) = parent.extensions_mut().get_mut::<Open>()
        {
            up.stages.push((name, total_ms));
        }
        if open.stages.is_empty() && !self.always.contains(&name) {
            return;
        }
        keep(Timed {
            name,
            fields: open.fields,
            stages: open.stages,
            total_ms,
            at: SystemTime::now(),
        });
    }
}

/// OTLP export for `service`: a layer exporting every span, with the
/// global meter's metrics exported beside it, when an endpoint is named
/// (`OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` or `OTEL_EXPORTER_OTLP_ENDPOINT`,
/// http/protobuf). `None` when none is, or the exporter could not be
/// made. The providers live for the process.
#[must_use]
pub fn otel<S>(
    service: &'static str,
) -> Option<tracing_opentelemetry::OpenTelemetryLayer<S, opentelemetry_sdk::trace::SdkTracer>>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    use opentelemetry::trace::TracerProvider as _;
    let named = |var: &str| std::env::var(var).is_ok_and(|v| !v.trim().is_empty());
    if !(named("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT") || named("OTEL_EXPORTER_OTLP_ENDPOINT")) {
        return None;
    }
    let resource = || {
        opentelemetry_sdk::Resource::builder()
            .with_service_name(service)
            .build()
    };
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .build()
        .ok()?;
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource())
        .build();
    let tracer = provider.tracer(service);
    opentelemetry::global::set_tracer_provider(provider);
    if let Ok(metrics) = opentelemetry_otlp::MetricExporter::builder()
        .with_http()
        .build()
    {
        opentelemetry::global::set_meter_provider(
            opentelemetry_sdk::metrics::SdkMeterProvider::builder()
                .with_periodic_exporter(metrics)
                .with_resource(resource())
                .build(),
        );
    }
    Some(tracing_opentelemetry::layer().with_tracer(tracer))
}

/// Log every panic through `tracing` (`tracing::error!`, with the thread
/// and where), then run the hook that was installed before it.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let at = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        tracing::error!(
            panic.thread = thread.name().unwrap_or("<unnamed>"),
            panic.at = %at,
            panic.what = %info,
            "panic"
        );
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt as _;

    #[test]
    fn a_span_keeps_its_steps_and_fields() {
        let subscriber = tracing_subscriber::registry().with(SpanRing::default());
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!(
                "work",
                work.kind = "ring-test",
                work.ok = tracing::field::Empty
            );
            {
                let _in = span.enter();
                let step = tracing::info_span!("work.read").entered();
                drop(step);
                let step = tracing::info_span!("work.edit").entered();
                drop(step);
                span.record("work.ok", true);
            }
            drop(span);
            // A span with no steps and not named `always` is not kept.
            drop(tracing::info_span!("noise"));
            let kept = recent(KEPT);
            assert!(kept.iter().all(|t| t.name != "noise"), "{kept:?}");
            let work = kept
                .iter()
                .find(|t| t.field("work.kind") == Some("ring-test"))
                .expect("the span kept");
            assert_eq!(work.name, "work");
            assert_eq!(work.field("work.ok"), Some("true"));
            let steps: Vec<&str> = work.stages.iter().map(|(k, _)| *k).collect();
            assert_eq!(steps, ["work.read", "work.edit"]);
            assert!(work.stage("work.read").is_some());
        });
    }

    #[test]
    fn an_action_is_kept_without_steps() {
        let subscriber = tracing_subscriber::registry().with(SpanRing::default());
        tracing::subscriber::with_default(subscriber, || {
            drop(tracing::info_span!("action", action.id = "RING_TEST"));
            assert!(
                recent(KEPT)
                    .iter()
                    .any(|t| t.name == "action" && t.field("action.id") == Some("RING_TEST"))
            );
        });
    }
}
