//! A release's spans go to the collector it baked in: `<base>/v1/traces`,
//! with its ingest token as the bearer, the build's resource on them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

/// One request as the collector saw it: its request line, headers
/// (lower-cased names) and body.
struct Seen {
    line: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

/// A collector on loopback that answers 200 to everything and hands each
/// request over.
fn collector() -> (String, mpsc::Receiver<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/otlp", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut headers = Vec::new();
            loop {
                let mut h = String::new();
                reader.read_line(&mut h).unwrap();
                let h = h.trim_end();
                if h.is_empty() {
                    break;
                }
                let (k, v) = h.split_once(':').unwrap();
                headers.push((k.trim().to_ascii_lowercase(), v.trim().to_owned()));
            }
            let len = headers
                .iter()
                .find(|(k, _)| k == "content-length")
                .map_or(0, |(_, v)| v.parse().unwrap());
            let mut body = vec![0; len];
            reader.read_exact(&mut body).unwrap();
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n");
            let _ = tx.send(Seen {
                line: line.trim_end().to_owned(),
                headers,
                body,
            });
        }
    });
    (base, rx)
}

#[test]
fn a_release_posts_to_its_baked_collector_with_the_token() {
    let (base, seen) = collector();
    let endpoint = studio_otel::Endpoint::Shipped {
        base,
        token: Some("ingest-token".into()),
    };
    tracing_subscriber::registry()
        .with(studio_otel::layer_to(&endpoint, "studio-otel-test", "26.11.4").expect("a layer"))
        .init();
    drop(tracing::error_span!("shipped.check"));
    studio_otel::flush();

    let traces = std::iter::from_fn(|| seen.recv_timeout(Duration::from_secs(10)).ok())
        .find(|r| r.line.contains("/v1/traces"))
        .expect("the spans posted");
    assert_eq!(traces.line, "POST /otlp/v1/traces HTTP/1.1");
    let header = |name: &str| {
        traces
            .headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(header("authorization"), Some("Bearer ingest-token"));
    assert_eq!(header("content-type"), Some("application/x-protobuf"));
    // The protobuf carries the resource's strings as they are.
    let body = String::from_utf8_lossy(&traces.body);
    for expected in [
        "studio-otel-test",
        "26.11.4",
        std::env::consts::OS,
        "shipped.check",
    ] {
        assert!(
            body.contains(expected),
            "{expected} missing from the request"
        );
    }
}
