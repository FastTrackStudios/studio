//! The page's own traces: how long Studio takes to start, to load a page's
//! data, to engrave a chart, and every call it makes to Drive, as
//! OpenTelemetry spans. Each call to Drive carries a W3C `traceparent`, so
//! the node's spans for it (its queries, its reads) join the same trace.
//!
//! In a browser, spans are batched and posted as OTLP/JSON to the node
//! (`<mount>/api/otlp/v1/traces`), which passes them to the collector it
//! exports to; a node exporting nothing answers 404 and the page stops
//! sending. Natively they are `tracing` spans, exported by app-shell's
//! telemetry like the rest of the app's.

#[cfg(target_arch = "wasm32")]
pub use browser::*;
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
mod browser {
    use std::cell::{Cell, RefCell};

    use serde_json::{Value, json};

    thread_local! {
        static DONE: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
        static OFF: Cell<bool> = const { Cell::new(false) };
        static FLUSHING: Cell<bool> = const { Cell::new(false) };
    }

    const SERVICE: &str = "studio-web";

    fn now_ns() -> u64 {
        (js_sys::Date::now() * 1_000_000.0) as u64
    }

    fn hex(bytes: usize) -> String {
        (0..bytes)
            .map(|_| format!("{:02x}", (js_sys::Math::random() * 256.0) as u8))
            .collect()
    }

    /// A span being timed; recorded when it ends (dropped).
    pub struct Span {
        trace: String,
        id: String,
        parent: String,
        name: String,
        kind: u8,
        start: u64,
        attrs: Vec<(String, Value)>,
        error: Option<String>,
    }

    impl Span {
        /// A new trace.
        pub fn root(name: &str) -> Span {
            Span::new(name, hex(16), String::new(), 1)
        }

        /// A span under this one.
        pub fn child(&self, name: &str) -> Span {
            Span::new(name, self.trace.clone(), self.id.clone(), 1)
        }

        /// A call out (to Drive), under this one.
        pub fn client(&self, name: &str) -> Span {
            Span::new(name, self.trace.clone(), self.id.clone(), 3)
        }

        fn new(name: &str, trace: String, parent: String, kind: u8) -> Span {
            Span {
                trace,
                id: hex(8),
                parent,
                name: name.to_string(),
                kind,
                start: now_ns(),
                attrs: Vec::new(),
                error: None,
            }
        }

        /// The start of the page itself (navigation), for the boot span.
        pub fn starting_at_navigation(mut self) -> Span {
            if let Some(origin) = web_sys::window()
                .and_then(|w| w.performance())
                .map(|p| p.time_origin())
            {
                self.start = (origin * 1_000_000.0) as u64;
            }
            self
        }

        pub fn set(&mut self, key: &str, value: impl Into<Value>) {
            self.attrs.push((key.to_string(), value.into()));
        }

        pub fn fail(&mut self, why: impl Into<String>) {
            self.error = Some(why.into());
        }

        /// The W3C header that makes the callee's spans this span's children.
        pub fn traceparent(&self) -> String {
            format!("00-{}-{}-01", self.trace, self.id)
        }
    }

    impl Drop for Span {
        fn drop(&mut self) {
            if OFF.with(Cell::get) {
                return;
            }
            let attributes: Vec<Value> = self
                .attrs
                .iter()
                .map(|(k, v)| {
                    let value = match v {
                        Value::Bool(b) => json!({ "boolValue": b }),
                        Value::Number(n) if n.is_i64() || n.is_u64() => {
                            json!({ "intValue": n.to_string() })
                        }
                        Value::Number(n) => json!({ "doubleValue": n.as_f64() }),
                        Value::String(s) => json!({ "stringValue": s }),
                        other => json!({ "stringValue": other.to_string() }),
                    };
                    json!({ "key": k, "value": value })
                })
                .collect();
            let status = match &self.error {
                Some(m) => json!({ "code": 2, "message": m }),
                None => json!({ "code": 1 }),
            };
            let span = json!({
                "traceId": self.trace,
                "spanId": self.id,
                "parentSpanId": self.parent,
                "name": self.name,
                "kind": self.kind,
                "startTimeUnixNano": self.start.to_string(),
                "endTimeUnixNano": now_ns().to_string(),
                "attributes": attributes,
                "status": status,
            });
            DONE.with(|d| d.borrow_mut().push(span));
            schedule();
        }
    }

    /// Send what has ended, a moment after the last span ends.
    fn schedule() {
        if FLUSHING.with(|f| f.replace(true)) {
            return;
        }
        wasm_bindgen_futures::spawn_local(async {
            gloo_timers::future::TimeoutFuture::new(1500).await;
            FLUSHING.with(|f| f.set(false));
            flush().await;
        });
    }

    async fn flush() {
        let spans = DONE.with(|d| std::mem::take(&mut *d.borrow_mut()));
        if spans.is_empty() || OFF.with(Cell::get) {
            return;
        }
        let body = json!({
            "resourceSpans": [{
                "resource": { "attributes": [
                    { "key": "service.name", "value": { "stringValue": SERVICE } },
                    { "key": "deployment.environment", "value": { "stringValue": "dev" } }
                ]},
                "scopeSpans": [{ "scope": { "name": "studio" }, "spans": spans }]
            }]
        })
        .to_string();
        let url = drive_dioxus::mount::api("/otlp/v1/traces");
        let sent = drive_dioxus::auth::fetch(
            "POST",
            &url,
            &[("content-type", "application/json")],
            Some(&body),
        )
        .await;
        // No collector behind the node: stop sending for this page.
        if matches!(sent, Some((404, _))) {
            OFF.with(|o| o.set(true));
        }
    }

    /// The span of the page itself: from navigation until the app has drawn.
    pub fn boot() {
        let mut s = Span::root("studio.boot").starting_at_navigation();
        s.set(
            "studio.path",
            web_sys::window()
                .and_then(|w| w.location().pathname().ok())
                .unwrap_or_default(),
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use serde_json::Value;
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;

    /// A span being timed: a `tracing` span, ended when this drops.
    pub struct Span {
        span: tracing::Span,
        attrs: Vec<(String, Value)>,
        error: Option<String>,
    }

    fn open(name: &str, kind: &str, parent: Option<&tracing::Span>) -> tracing::Span {
        match parent {
            Some(p) => tracing::info_span!(
                parent: p,
                "studio",
                otel.name = %name,
                otel.kind = kind,
                studio.attrs = tracing::field::Empty,
                otel.status_code = tracing::field::Empty,
                otel.status_message = tracing::field::Empty,
            ),
            None => tracing::info_span!(
                parent: None,
                "studio",
                otel.name = %name,
                otel.kind = kind,
                studio.attrs = tracing::field::Empty,
                otel.status_code = tracing::field::Empty,
                otel.status_message = tracing::field::Empty,
            ),
        }
    }

    impl Span {
        /// A new trace.
        pub fn root(name: &str) -> Span {
            Self::of(open(name, "internal", None))
        }

        /// A span under this one.
        pub fn child(&self, name: &str) -> Span {
            Self::of(open(name, "internal", Some(&self.span)))
        }

        /// A call out (to Drive), under this one.
        pub fn client(&self, name: &str) -> Span {
            Self::of(open(name, "client", Some(&self.span)))
        }

        fn of(span: tracing::Span) -> Span {
            Span {
                span,
                attrs: Vec::new(),
                error: None,
            }
        }

        /// The app's own start has no navigation to date it from.
        #[allow(dead_code)]
        pub fn starting_at_navigation(self) -> Span {
            self
        }

        pub fn set(&mut self, key: &str, value: impl Into<Value>) {
            self.attrs.push((key.to_string(), value.into()));
        }

        pub fn fail(&mut self, why: impl Into<String>) {
            self.error = Some(why.into());
        }

        /// The W3C header that makes the callee's spans this span's children.
        pub fn traceparent(&self) -> String {
            use opentelemetry::trace::TraceContextExt as _;
            let cx = self.span.context();
            let sc = cx.span().span_context().clone();
            format!("00-{}-{}-01", sc.trace_id(), sc.span_id())
        }
    }

    impl Drop for Span {
        fn drop(&mut self) {
            if !self.attrs.is_empty() {
                let attrs: serde_json::Map<String, Value> = self.attrs.drain(..).collect();
                self.span
                    .record("studio.attrs", Value::Object(attrs).to_string());
            }
            if let Some(why) = &self.error {
                self.span.record("otel.status_code", "ERROR");
                self.span.record("otel.status_message", why.as_str());
            }
        }
    }

    /// The app's start: how long from the process starting to the first draw.
    pub fn boot() {
        let mut s = Span::root("studio.boot");
        s.set("studio.startup_ms", crate::storage::since_start_ms());
    }
}
