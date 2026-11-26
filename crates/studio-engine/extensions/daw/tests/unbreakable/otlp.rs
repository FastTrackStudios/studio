//! A local OTLP receiver the test owns: the engine exports its spans
//! here (http/protobuf, `OTEL_EXPORTER_OTLP_ENDPOINT`), never to the
//! owner's collector. Spans are kept in memory, in arrival order, for the
//! steps to wait on and the report to break down.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{AnyValue, any_value};
use prost::Message as _;
use serde_json::Value;

/// One span the engine exported.
#[derive(Clone, Debug)]
pub struct Span {
    pub name: String,
    pub trace: Vec<u8>,
    pub id: Vec<u8>,
    pub parent: Vec<u8>,
    pub start_ns: u64,
    pub end_ns: u64,
    pub attrs: BTreeMap<String, Value>,
}

impl Span {
    /// How long it lasted.
    #[must_use]
    pub fn ms(&self) -> f64 {
        self.end_ns.saturating_sub(self.start_ns) as f64 / 1e6
    }

    /// A field, as a number.
    #[must_use]
    pub fn number(&self, key: &str) -> Option<f64> {
        self.attrs.get(key).and_then(Value::as_f64)
    }

    /// A field, as text.
    #[must_use]
    pub fn text(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).and_then(Value::as_str)
    }
}

#[derive(Default)]
struct Store {
    spans: Vec<Span>,
    /// Requests whose body could not be read (compressed, cut).
    unreadable: usize,
}

/// The receiver: its endpoint and what it has received.
#[derive(Clone)]
pub struct Receiver {
    pub endpoint: String,
    store: Arc<(Mutex<Store>, Condvar)>,
}

impl Receiver {
    /// Listening on a free port of loopback, on threads of its own for
    /// as long as the process runs.
    ///
    /// # Panics
    ///
    /// Loopback could not be bound.
    #[must_use]
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port for OTLP");
        let endpoint = format!("http://{}", listener.local_addr().expect("its address"));
        let store: Arc<(Mutex<Store>, Condvar)> = Arc::default();
        let kept = store.clone();
        std::thread::Builder::new()
            .name("otlp-accept".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let kept = kept.clone();
                    let _ = std::thread::Builder::new()
                        .name("otlp-conn".into())
                        .spawn(move || serve(stream, &kept));
                }
            })
            .expect("the OTLP thread");
        Self { endpoint, store }
    }

    /// Every span so far.
    #[must_use]
    pub fn spans(&self) -> Vec<Span> {
        lock(&self.store.0).spans.clone()
    }

    /// Requests that could not be decoded.
    #[must_use]
    pub fn unreadable(&self) -> usize {
        lock(&self.store.0).unreadable
    }

    /// The first span received from `from` on (an index into
    /// [`Self::spans`]) that `matches`, waiting up to `wait` for it.
    #[must_use]
    pub fn wait_for(
        &self,
        from: usize,
        wait: Duration,
        matches: impl Fn(&Span) -> bool,
    ) -> Option<Span> {
        let deadline = Instant::now() + wait;
        let (store, arrived) = &*self.store;
        let mut held = lock(store);
        loop {
            if let Some(span) = held.spans.iter().skip(from).find(|s| matches(s)) {
                return Some(span.clone());
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            held = arrived
                .wait_timeout(held, left)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }

    /// How many spans have arrived: a mark for [`Self::wait_for`].
    #[must_use]
    pub fn mark(&self) -> usize {
        lock(&self.store.0).spans.len()
    }
}

fn lock(store: &Mutex<Store>) -> std::sync::MutexGuard<'_, Store> {
    store
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// One connection: HTTP/1.1 requests one after another (the exporter
/// keeps its connection), each answered 200 with no body.
fn serve(stream: TcpStream, store: &(Mutex<Store>, Condvar)) {
    let Ok(mut write) = stream.try_clone() else {
        return;
    };
    let mut read = BufReader::new(stream);
    loop {
        let mut line = String::new();
        if read.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let path = line
            .split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .to_owned();
        let mut length = 0usize;
        let mut encoded = false;
        loop {
            let mut header = String::new();
            if read.read_line(&mut header).unwrap_or(0) == 0 {
                return;
            }
            let header = header.trim_end();
            if header.is_empty() {
                break;
            }
            let (name, value) = header.split_once(':').unwrap_or((header, ""));
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0);
            } else if name.eq_ignore_ascii_case("content-encoding") && value.trim() != "identity" {
                encoded = true;
            }
        }
        let mut body = vec![0; length];
        if read.read_exact(&mut body).is_err() {
            return;
        }
        if path.starts_with("/v1/traces") {
            let spans = (!encoded).then(|| decode(&body)).flatten();
            let (held, arrived) = store;
            let mut held = lock(held);
            match spans {
                Some(spans) => held.spans.extend(spans),
                None => held.unreadable += 1,
            }
            arrived.notify_all();
        }
        let reply =
            "HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 0\r\n\r\n";
        if write.write_all(reply.as_bytes()).is_err() {
            return;
        }
    }
}

/// The spans of one export request; `None` when it is not one.
#[must_use]
pub fn decode(body: &[u8]) -> Option<Vec<Span>> {
    let request = ExportTraceServiceRequest::decode(body).ok()?;
    let spans = request
        .resource_spans
        .into_iter()
        .flat_map(|r| r.scope_spans)
        .flat_map(|s| s.spans)
        .map(|s| Span {
            name: s.name,
            trace: s.trace_id,
            id: s.span_id,
            parent: s.parent_span_id,
            start_ns: s.start_time_unix_nano,
            end_ns: s.end_time_unix_nano,
            attrs: s
                .attributes
                .into_iter()
                .map(|kv| (kv.key, kv.value.map_or(Value::Null, json)))
                .collect(),
        })
        .collect();
    Some(spans)
}

fn json(value: AnyValue) -> Value {
    match value.value {
        Some(any_value::Value::StringValue(s)) => Value::String(s),
        Some(any_value::Value::BoolValue(b)) => Value::Bool(b),
        Some(any_value::Value::IntValue(i)) => Value::from(i),
        Some(any_value::Value::DoubleValue(d)) => {
            serde_json::Number::from_f64(d).map_or(Value::Null, Value::Number)
        }
        Some(any_value::Value::ArrayValue(a)) => {
            Value::Array(a.values.into_iter().map(json).collect())
        }
        Some(other) => Value::String(format!("{other:?}")),
        None => Value::Null,
    }
}

/// Where the time of one unit of work went: every span of `root`'s trace
/// that lies under it, by name — how many, their total, the longest, and
/// their self time (what no span under them accounts for: where the trace
/// stops saying where the time went), longest total first.
#[must_use]
pub fn breakdown(spans: &[Span], root: &Span) -> Vec<Value> {
    let under: Vec<&Span> = spans
        .iter()
        .filter(|s| s.trace == root.trace && is_under(spans, s, root))
        .collect();
    let mut children: HashMap<&[u8], Vec<&Span>> = HashMap::new();
    for span in &under {
        children
            .entry(span.parent.as_slice())
            .or_default()
            .push(span);
    }
    let mut by_name: BTreeMap<&str, (usize, f64, f64, f64, Option<&Span>)> = BTreeMap::new();
    for span in &under {
        let own = children
            .get(span.id.as_slice())
            .map_or(&[][..], Vec::as_slice);
        let row = by_name.entry(span.name.as_str()).or_default();
        row.0 += 1;
        row.1 += span.ms();
        if span.ms() >= row.2 {
            row.2 = span.ms();
            row.4 = Some(span);
        }
        row.3 += self_ns(span, own) as f64 / 1e6;
    }
    let mut rows: Vec<_> = by_name.into_iter().collect();
    rows.sort_by(|a, b| b.1.1.total_cmp(&a.1.1));
    rows.into_iter()
        .map(|(name, (count, total, max, own, longest))| {
            serde_json::json!({
                "name": name,
                "count": count,
                "total_ms": round(total),
                "max_ms": round(max),
                "self_ms": round(own),
                "fields": longest.map(fields_of).unwrap_or_default(),
            })
        })
        .collect()
}

/// What a span says about its work (`store.updates`, `fx.count`, …): its
/// fields, without the exporter's own (where in the code, which thread,
/// busy and idle time).
fn fields_of(span: &Span) -> serde_json::Map<String, Value> {
    span.attrs
        .iter()
        .filter(|(k, _)| {
            !(k.starts_with("code.")
                || k.starts_with("thread.")
                || k.starts_with("otel.")
                || k.as_str() == "busy_ns"
                || k.as_str() == "idle_ns"
                || k.as_str() == "target")
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// The part of `span` none of its `children` covers (their union, clipped
/// to it: children running at once count once).
fn self_ns(span: &Span, children: &[&Span]) -> u64 {
    let mut parts: Vec<(u64, u64)> = children
        .iter()
        .map(|c| (c.start_ns.max(span.start_ns), c.end_ns.min(span.end_ns)))
        .filter(|(from, to)| to > from)
        .collect();
    parts.sort_unstable();
    let mut covered = 0;
    let mut run: Option<(u64, u64)> = None;
    for (from, to) in parts {
        run = match run {
            Some((start, end)) if from <= end => Some((start, end.max(to))),
            Some((start, end)) => {
                covered += end - start;
                Some((from, to))
            }
            None => Some((from, to)),
        };
    }
    if let Some((start, end)) = run {
        covered += end - start;
    }
    span.end_ns
        .saturating_sub(span.start_ns)
        .saturating_sub(covered)
}

/// Whether `span` is `root` or below it.
fn is_under(spans: &[Span], span: &Span, root: &Span) -> bool {
    let mut at = span;
    for _ in 0..64 {
        if at.id == root.id {
            return true;
        }
        match spans
            .iter()
            .find(|s| s.id == at.parent && s.trace == at.trace)
        {
            Some(parent) => at = parent,
            None => return false,
        }
    }
    false
}

/// The outermost span of `span`'s trace above it.
#[must_use]
pub fn root_of<'a>(spans: &'a [Span], span: &'a Span) -> &'a Span {
    let mut at = span;
    for _ in 0..64 {
        match spans
            .iter()
            .find(|s| s.id == at.parent && s.trace == at.trace)
        {
            Some(parent) => at = parent,
            None => break,
        }
    }
    at
}

/// `ms` to a tenth of a millisecond.
#[must_use]
pub fn round(ms: f64) -> f64 {
    (ms * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry_proto::tonic::common::v1::KeyValue;
    use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span as ProtoSpan};

    fn span(name: &str, id: u8, parent: u8, start: u64, end: u64) -> ProtoSpan {
        ProtoSpan {
            trace_id: vec![7; 16],
            span_id: vec![id; 8],
            parent_span_id: if parent == 0 {
                Vec::new()
            } else {
                vec![parent; 8]
            },
            name: name.to_owned(),
            start_time_unix_nano: start,
            end_time_unix_nano: end,
            attributes: vec![KeyValue {
                key: "song.ready_ms".into(),
                value: Some(AnyValue {
                    value: Some(any_value::Value::DoubleValue(12.5)),
                }),
            }],
            ..ProtoSpan::default()
        }
    }

    fn request(spans: Vec<ProtoSpan>) -> Vec<u8> {
        ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                scope_spans: vec![ScopeSpans {
                    spans,
                    ..ScopeSpans::default()
                }],
                ..ResourceSpans::default()
            }],
        }
        .encode_to_vec()
    }

    #[test]
    fn an_export_is_read_back_with_its_fields() {
        let spans = decode(&request(vec![span("song.open", 1, 0, 0, 3_000_000)])).unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "song.open");
        assert!((spans[0].ms() - 3.0).abs() < 1e-9);
        assert_eq!(spans[0].number("song.ready_ms"), Some(12.5));
    }

    #[test]
    fn a_breakdown_counts_only_what_lies_under_its_root() {
        let spans = decode(&request(vec![
            span("set.open", 1, 0, 0, 10_000_000),
            span("song.open", 2, 1, 0, 9_000_000),
            span("fx.load_states", 3, 2, 1_000_000, 5_000_000),
            span("fx.load_states", 4, 2, 5_000_000, 6_000_000),
            span("elsewhere", 5, 1, 0, 1_000_000),
        ]))
        .unwrap();
        let open = spans.iter().find(|s| s.name == "song.open").unwrap();
        let rows = breakdown(&spans, open);
        let names: Vec<&str> = rows.iter().map(|r| r["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["song.open", "fx.load_states"]);
        assert_eq!(rows[1]["count"], 2);
        assert_eq!(rows[1]["total_ms"], 5.0);
        assert_eq!(rows[1]["max_ms"], 4.0);
        // 9 ms, of which its two children cover 1–6 ms: 4 ms are its own.
        assert_eq!(rows[0]["self_ms"], 4.0);
        assert_eq!(rows[1]["self_ms"], 5.0);
        assert_eq!(root_of(&spans, open).name, "set.open");
    }

    #[test]
    fn children_at_once_cover_their_parent_once() {
        let spans = decode(&request(vec![
            span("song.open", 1, 0, 0, 10_000_000),
            span("fx.load", 2, 1, 1_000_000, 6_000_000),
            span("fx.load", 3, 1, 4_000_000, 8_000_000),
            span("past.its.end", 4, 1, 9_000_000, 12_000_000),
        ]))
        .unwrap();
        let open = spans.iter().find(|s| s.name == "song.open").unwrap();
        let rows = breakdown(&spans, open);
        let own = |name: &str| {
            rows.iter()
                .find(|r| r["name"] == name)
                .and_then(|r| r["self_ms"].as_f64())
        };
        // Covered 1–8 and 9–10 of its 10 ms.
        assert_eq!(own("song.open"), Some(2.0));
    }

    #[test]
    fn the_receiver_takes_what_an_exporter_posts() {
        let receiver = Receiver::start();
        let body = request(vec![span("transport.play", 9, 0, 0, 70_000_000)]);
        let at = receiver.endpoint.trim_start_matches("http://").to_owned();
        let mut stream = TcpStream::connect(at).unwrap();
        let head = format!(
            "POST /v1/traces HTTP/1.1\r\nHost: x\r\nContent-Type: application/x-protobuf\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        stream.write_all(head.as_bytes()).unwrap();
        stream.write_all(&body).unwrap();
        let got = receiver
            .wait_for(0, Duration::from_secs(5), |s| s.name == "transport.play")
            .expect("the span arrives");
        assert!((got.ms() - 70.0).abs() < 1e-9);
    }
}
