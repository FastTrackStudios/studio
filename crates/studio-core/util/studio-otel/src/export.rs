//! The native exporter: spans and metrics over OTLP (http/protobuf), and
//! the crash hook that flushes them before a panic takes the process.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{WithExportConfig as _, WithHttpConfig as _};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{SdkTracer, SdkTracerProvider};
use tracing::Subscriber;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::registry::LookupSpan;

use crate::{Crash, Endpoint, resource_attributes};

/// The tracer provider [`layer`] or [`adopt`] made, for [`flush`].
static PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();

/// The thread the SDK's batch span processor runs on: a panic there must
/// not wait on itself to flush.
const BATCH_THREAD: &str = "OpenTelemetry.Traces.BatchProcessor";

fn resource(service: &str, version: &str) -> Resource {
    Resource::builder()
        .with_attributes(
            resource_attributes(service, version)
                .into_iter()
                .map(|(k, v)| KeyValue::new(k, v)),
        )
        .build()
}

fn bearer(token: Option<&String>) -> HashMap<String, String> {
    token
        .map(|t| HashMap::from([("Authorization".to_owned(), format!("Bearer {t}"))]))
        .unwrap_or_default()
}

/// OTLP export for `service` at `version`, when [`Endpoint`] names a
/// collector.
///
/// A layer exporting every span, with the global meter's metrics exported
/// beside it. `None` when no collector is named, or the exporter could not
/// be made. The providers live for the process; [`flush`] sends what is
/// buffered.
#[must_use]
pub fn layer<S>(service: &'static str, version: &str) -> Option<OpenTelemetryLayer<S, SdkTracer>>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    layer_to(&Endpoint::resolve()?, service, version)
}

/// [`layer`], to `endpoint`.
#[must_use]
pub fn layer_to<S>(
    endpoint: &Endpoint,
    service: &'static str,
    version: &str,
) -> Option<OpenTelemetryLayer<S, SdkTracer>>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let spans = opentelemetry_otlp::SpanExporter::builder().with_http();
    let spans = match endpoint {
        Endpoint::Environment => spans,
        Endpoint::Shipped { base, token } => spans
            .with_endpoint(format!("{base}/v1/traces"))
            .with_headers(bearer(token.as_ref())),
    };
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(spans.build().ok()?)
        .with_resource(resource(service, version))
        .build();
    let metrics = opentelemetry_otlp::MetricExporter::builder().with_http();
    let metrics = match endpoint {
        Endpoint::Environment => metrics,
        Endpoint::Shipped { base, token } => metrics
            .with_endpoint(format!("{base}/v1/metrics"))
            .with_headers(bearer(token.as_ref())),
    };
    if let Ok(metrics) = metrics.build() {
        opentelemetry::global::set_meter_provider(
            opentelemetry_sdk::metrics::SdkMeterProvider::builder()
                .with_periodic_exporter(metrics)
                .with_resource(resource(service, version))
                .build(),
        );
    }
    Some(adopt(provider))
}

/// The layer for a provider made elsewhere (a test's in-memory one), kept
/// as the process's: global, and what [`flush`] flushes. The first
/// provider adopted stays the one [`flush`] reaches.
#[must_use]
pub fn adopt<S>(provider: SdkTracerProvider) -> OpenTelemetryLayer<S, SdkTracer>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let tracer = provider.tracer("studio");
    let _ = PROVIDER.set(provider.clone());
    opentelemetry::global::set_tracer_provider(provider);
    tracing_opentelemetry::layer().with_tracer(tracer)
}

/// Set `key` to `value` among the current span's exported attributes: a
/// field of the wide event known only where it is computed, on a span
/// opened elsewhere (the RPC span the framework makes per call). A span
/// may only record fields it declared; this adds any. Nothing when no
/// span is exported.
pub fn set(key: impl Into<opentelemetry::Key>, value: impl Into<opentelemetry::Value>) {
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;
    tracing::Span::current().set_attribute(key, value);
}

/// Send the spans buffered so far, waiting for the collector (bounded by
/// the exporter's timeout). Nothing when no provider was made.
pub fn flush() {
    if let Some(provider) = PROVIDER.get() {
        let _ = provider.force_flush();
    }
}

/// Every panic becomes a [`Crash`] span, flushed to the collector.
///
/// Then the hook installed before this one runs, so the process's usual
/// report still prints, and a `panic = "abort"` build still aborts after it.
///
/// A panic inside the hook, or on the exporter's own thread, is passed on
/// without another report.
pub fn install_crash_hook() {
    static REPORTING: AtomicBool = AtomicBool::new(false);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if !REPORTING.swap(true, Ordering::SeqCst) {
            Crash::of(info).record();
            if std::thread::current().name() != Some(BATCH_THREAD) {
                flush();
            }
            REPORTING.store(false, Ordering::SeqCst);
        }
        previous(info);
    }));
}
