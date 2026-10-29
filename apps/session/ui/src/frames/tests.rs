use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData, ResourceMetrics};
use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};

use super::Frame;
use super::metrics::Instruments;

#[test]
fn each_frame_is_a_sample_and_slow_ones_are_counted() {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(exporter.clone()).build())
        .build();
    let at = Instruments::new(&provider.meter("session-ui"));
    // Two frames the shell timed, one inside the 240 Hz budget and one
    // over it; one only the paint timed (no shell), inside it.
    let frame = |paint, shell| Frame {
        paint,
        shell,
        ..Frame::default()
    };
    at.record(&frame(1_000, 3_000));
    at.record(&frame(2_000, 9_000));
    at.record(&frame(2_500, 0));
    provider.force_flush().expect("flush");

    let finished = exporter.get_finished_metrics().expect("metrics");
    let metrics: Vec<_> = finished
        .iter()
        .flat_map(ResourceMetrics::scope_metrics)
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
        .collect();
    let named = |name: &str| {
        metrics
            .iter()
            .find(|m| m.name() == name)
            .unwrap_or_else(|| panic!("{name}"))
    };

    let duration = named("ui.frame.duration");
    assert_eq!(duration.unit(), "ms");
    let AggregatedMetrics::F64(MetricData::Histogram(h)) = duration.data() else {
        panic!("a histogram");
    };
    let mut by_source: Vec<(String, u64, f64)> = h
        .data_points()
        .map(|p| {
            let source = p
                .attributes()
                .find(|kv| kv.key.as_str() == "ui.frame.source")
                .map(|kv| kv.value.to_string())
                .unwrap_or_default();
            (source, p.count(), p.sum())
        })
        .collect();
    by_source.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        by_source,
        vec![("paint".to_owned(), 1, 2.5), ("shell".to_owned(), 2, 12.0)]
    );

    let AggregatedMetrics::F64(MetricData::Histogram(paint)) = named("ui.paint.duration").data()
    else {
        panic!("a histogram");
    };
    assert_eq!(paint.data_points().map(|p| p.count()).sum::<u64>(), 3);

    let AggregatedMetrics::U64(MetricData::Sum(over)) = named("ui.frame.over_budget").data() else {
        panic!("a counter");
    };
    assert_eq!(over.data_points().map(|p| p.value()).sum::<u64>(), 1);

    // Observed from the frame log when the meter collects.
    let fps = named("ui.frames_per_second");
    assert!(matches!(
        fps.data(),
        AggregatedMetrics::F64(MetricData::Gauge(_))
    ));
}

#[test]
fn late_frames_are_counted_against_each_rate_and_rest_is_not_a_frame() {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(exporter.clone()).build())
        .build();
    let at = Instruments::new(&provider.meter("session-ui"));
    let after = |since| Frame {
        since,
        paint: 1_000,
        ..Frame::default()
    };
    // On time at 120 Hz; late for 120 but not 60; late for both; the
    // window at rest (not a frame); the first paint (no gap).
    for since in [9_000, 12_000, 30_000, 300_000, 0] {
        at.record(&after(since));
    }
    provider.force_flush().expect("flush");

    let finished = exporter.get_finished_metrics().expect("metrics");
    let metrics: Vec<_> = finished
        .iter()
        .flat_map(ResourceMetrics::scope_metrics)
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
        .collect();
    let named = |name: &str| {
        metrics
            .iter()
            .find(|m| m.name() == name)
            .unwrap_or_else(|| panic!("{name}"))
    };

    let AggregatedMetrics::F64(MetricData::Histogram(gaps)) = named("ui.frame.interval").data()
    else {
        panic!("a histogram");
    };
    assert_eq!(gaps.data_points().map(|p| p.count()).sum::<u64>(), 3);

    let AggregatedMetrics::U64(MetricData::Sum(missed)) = named("ui.frame.missed").data() else {
        panic!("a counter");
    };
    let mut by_rate: Vec<(String, u64)> = missed
        .data_points()
        .map(|p| {
            let hz = p
                .attributes()
                .find(|kv| kv.key.as_str() == "ui.budget_hz")
                .map(|kv| kv.value.to_string())
                .unwrap_or_default();
            (hz, p.value())
        })
        .collect();
    by_rate.sort();
    assert_eq!(by_rate, vec![("120".to_owned(), 2), ("60".to_owned(), 1)]);
}

#[test]
fn the_low_rate_is_the_slowest_hundredth_of_the_gaps() {
    use super::metrics::low_fps;
    // Ninety-nine frames at 120 Hz and one 50 ms stall: the stall is the
    // 1% low, 20 frames a second.
    let mut gaps = vec![8_333_u64; 99];
    gaps.push(50_000);
    assert!((low_fps(gaps) - 20.0).abs() < 0.01);
    assert!(low_fps(Vec::new()).abs() < f64::EPSILON);
}

#[test]
fn a_late_frame_is_a_trace_of_what_the_paint_thread_did_in_it() {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider};
    use tracing_subscriber::layer::SubscriberExt as _;

    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    opentelemetry::global::set_tracer_provider(provider.clone());
    let subscriber = tracing_subscriber::registry().with(super::slow::WorkLayer);
    tracing::subscriber::with_default(subscriber, || {
        // A paint on time: this thread paints, nothing is exported.
        super::slow::painted(&Frame {
            since: 8_000,
            ..Frame::default()
        });
        // Work in the gap, then a paint 30 ms after the one before.
        tracing::info_span!("window.resync").in_scope(|| {
            tracing::info_span!("adopt").in_scope(|| {
                std::thread::sleep(std::time::Duration::from_millis(2));
            });
        });
        super::slow::painted(&Frame {
            since: 30_000,
            paint: 1_500,
            ..Frame::default()
        });
    });
    provider.force_flush().expect("flush");
    let spans = exporter.get_finished_spans().expect("spans");
    let named = |name: &str| {
        spans
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("{name} in {spans:#?}"))
    };
    let frame = named("ui.frame.slow");
    let resync = named("window.resync");
    let adopt = named("adopt");
    assert_eq!(
        spans.iter().filter(|s| s.name == "ui.frame.slow").count(),
        1
    );
    assert_eq!(resync.parent_span_id, frame.span_context.span_id());
    assert_eq!(adopt.parent_span_id, resync.span_context.span_id());
    let cause = frame
        .attributes
        .iter()
        .find(|kv| kv.key.as_str() == "ui.frame.cause")
        .map(|kv| kv.value.to_string());
    assert_eq!(cause.as_deref(), Some("window.resync"));
}
