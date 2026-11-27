---
"keys" = "patch"
"sampler" = "patch"
"studio-otel" = "patch"
---

Signal's keys rig and sampler enrich their RPC spans through studio_otel::set on plain tracing + OpenTelemetry; architect-telemetry is gone from the workspace.
