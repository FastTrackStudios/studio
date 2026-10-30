---
"session-desktop" = "patch"
"studio-desktop" = "patch"
---

`just dev [local]` runs the whole loop in a herdr workspace — the engine, the window under dx, a local Task server, an OpenTelemetry Collector and teley — and opens teley's dashboard as a window of its own; the `studio` CLI and its engine export traces, logs and metrics when OTEL_EXPORTER_OTLP_ENDPOINT is set, the terminals then keeping warnings and errors (session)
