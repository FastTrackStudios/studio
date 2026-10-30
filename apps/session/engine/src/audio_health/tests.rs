use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::metrics::data::{AggregatedMetrics, Metric, MetricData, ResourceMetrics};
use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};

use super::Health;
use super::metrics::Instruments;

/// A metric's points as (`audio.stat` or "", value).
fn points(metric: &Metric) -> Vec<(String, f64)> {
    fn stat<'a>(mut kvs: impl Iterator<Item = &'a opentelemetry::KeyValue>) -> String {
        kvs.find(|kv| kv.key.as_str() == "audio.stat")
            .map(|kv| kv.value.to_string())
            .unwrap_or_default()
    }
    let mut out: Vec<(String, f64)> = match metric.data() {
        AggregatedMetrics::F64(MetricData::Gauge(g)) => g
            .data_points()
            .map(|p| (stat(p.attributes()), p.value()))
            .collect(),
        AggregatedMetrics::F64(MetricData::Sum(s)) => s
            .data_points()
            .map(|p| (stat(p.attributes()), p.value()))
            .collect(),
        AggregatedMetrics::U64(MetricData::Gauge(g)) => g
            .data_points()
            .map(|p| (stat(p.attributes()), p.value() as f64))
            .collect(),
        AggregatedMetrics::U64(MetricData::Sum(s)) => s
            .data_points()
            .map(|p| (stat(p.attributes()), p.value() as f64))
            .collect(),
        other => panic!("{}: {other:?}", metric.name()),
    };
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn an_interval_is_recorded_as_load_counters_and_the_render_ahead() {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(exporter.clone()).build())
        .build();
    let at = Instruments::new(&provider.meter("session-engine"));
    let interval = Health {
        blocks: 375,
        frames: 256,
        budget_ms: 5.0,
        mean_ms: 1.0,
        peak_ms: 4.0,
        over_budget: 1,
        xruns: 2,
        ahead_ms: 200.0,
        ahead_aim_ms: 250.0,
        ahead_underruns: 3,
        ahead_restarts: 1,
        ahead_rewinds: 4,
        ahead_stale_ms: 12.5,
        ahead_load: 0.25,
    };
    at.record(&interval);
    at.record(&interval);
    provider.force_flush().expect("flush");

    let finished = exporter.get_finished_metrics().expect("metrics");
    let metrics: Vec<&Metric> = finished
        .iter()
        .flat_map(ResourceMetrics::scope_metrics)
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
        .collect();
    let named = |name: &str| {
        let m = metrics
            .iter()
            .find(|m| m.name() == name)
            .unwrap_or_else(|| panic!("{name}"));
        points(m)
    };
    let one = |v: f64| vec![(String::new(), v)];
    let by_stat = |mean: f64, max: f64| vec![("max".to_owned(), max), ("mean".to_owned(), mean)];

    // Gauges: the last interval's.
    assert_eq!(named("audio.render.duration"), by_stat(1.0, 4.0));
    assert_eq!(named("audio.load"), by_stat(0.2, 0.8));
    assert_eq!(named("audio.block.budget"), one(5.0));
    assert_eq!(named("audio.block.size"), one(256.0));
    assert_eq!(named("audio.render_ahead.depth"), one(200.0));
    assert_eq!(named("audio.render_ahead.aim"), one(250.0));
    assert_eq!(named("audio.render_ahead.load"), one(0.25));
    // Counters: both intervals'.
    assert_eq!(named("audio.blocks"), one(750.0));
    assert_eq!(named("audio.overruns"), one(2.0));
    assert_eq!(named("audio.xruns"), one(4.0));
    assert_eq!(named("audio.render_ahead.short"), one(6.0));
    assert_eq!(named("audio.render_ahead.restarts"), one(2.0));
    assert_eq!(named("audio.render_ahead.rewinds"), one(8.0));
    assert_eq!(named("audio.render_ahead.stale"), one(25.0));
}
