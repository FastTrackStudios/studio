---
"daw-extension" = "patch"
"engine-core" = "patch"
"studio-app" = "patch"
"studio-otel" = "patch"
"xtask-workflows" = "patch"
---

Crashes and errors reach telemetry: a panic in the app, the engine or a page in a browser is a `crash` span flushed before the process goes, every span says its version and platform, and a release build sends to the organization's collector baked in at build time (`Studio --crash-test` crashes on purpose) (studio-otel)
