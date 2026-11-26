//! A panic leaves a `crash` span, exported before the hook that was there
//! runs: through a batch exporter, so it is there only if the hook flushed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use opentelemetry::Value;
use opentelemetry::trace::Status;
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider};
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

#[test]
fn a_panic_is_exported_as_a_crash_span_before_the_process_goes() {
    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter.clone())
        .build();
    tracing_subscriber::registry()
        .with(studio_otel::adopt(provider))
        .init();
    studio_otel::install_crash_hook();

    let unwound = std::thread::Builder::new()
        .name("crash-test".into())
        .spawn(|| panic!("forced crash for the test"))
        .unwrap()
        .join();
    assert!(unwound.is_err());

    // No flush here: the hook's own flush is what put the span there.
    let spans = exporter.get_finished_spans().unwrap();
    let crash = spans
        .iter()
        .find(|s| s.name == "crash")
        .unwrap_or_else(|| panic!("no crash span among {spans:?}"));
    let attr = |key: &str| {
        crash
            .attributes
            .iter()
            .find(|kv| kv.key.as_str() == key)
            .map(|kv| kv.value.clone())
    };
    assert_eq!(attr("crash.thread"), Some(Value::from("crash-test")));
    let message = attr("crash.message")
        .expect("crash.message")
        .as_str()
        .into_owned();
    assert!(message.contains("forced crash for the test"), "{message}");
    let at = attr("crash.at").expect("crash.at").as_str().into_owned();
    assert!(at.contains("crash.rs:"), "{at}");
    assert!(
        matches!(&crash.status, Status::Error { description } if description.contains("forced crash")),
        "{:?}",
        crash.status
    );
}
