//! `set` enriches a span someone else opened (an RPC span the framework
//! made, say) with fields that span never declared, and they reach the
//! exporter as the span's attributes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use opentelemetry::Value;
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider};
use tracing_subscriber::layer::SubscriberExt as _;

#[test]
fn a_field_set_on_the_current_span_is_exported_with_it() {
    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry().with(studio_otel::adopt(provider));

    tracing::subscriber::with_default(subscriber, || {
        let _rpc = tracing::info_span!("rpc").entered();
        studio_otel::set("pack.name", "Rhodes".to_owned());
        studio_otel::set("pack.range_len", 4096_i64);
        studio_otel::set("pack.plan_cached", true);
        studio_otel::set("audio.render_mean_ms", 0.5_f64);
    });

    let spans = exporter.get_finished_spans().unwrap();
    let rpc = spans
        .iter()
        .find(|s| s.name == "rpc")
        .unwrap_or_else(|| panic!("no rpc span among {spans:?}"));
    let attr = |key: &str| {
        rpc.attributes
            .iter()
            .find(|kv| kv.key.as_str() == key)
            .map(|kv| kv.value.clone())
    };
    assert_eq!(attr("pack.name"), Some(Value::from("Rhodes")));
    assert_eq!(attr("pack.range_len"), Some(Value::from(4096_i64)));
    assert_eq!(attr("pack.plan_cached"), Some(Value::from(true)));
    assert_eq!(attr("audio.render_mean_ms"), Some(Value::from(0.5_f64)));
}

#[test]
fn with_no_subscriber_set_is_harmless() {
    let _span = tracing::info_span!("rpc").entered();
    studio_otel::set("pack.name", "Rhodes".to_owned());
}
