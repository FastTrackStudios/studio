---
"daw-extension" = "none"
"daw-standalone" = "none"
"room" = "none"
"session" = "none"
---

Tests: the per-song lock test checks ordering under a held lock instead of elapsed time; the lead, room-net and room tests wait on the state they assert; the opentelemetry_sdk batch counter's overflow panic is off in dev builds.
