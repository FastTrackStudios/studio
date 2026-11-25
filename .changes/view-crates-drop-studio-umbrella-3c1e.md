---
"arrangement-view" = "none"
"daw-view-kit" = "none"
"editor" = "none"
"mixer-view" = "none"
---

The DAW view crates no longer depend on studio-adapters' `studio` umbrella crate, editor pulls it only for its REAPER feature, and the kit drops its unused re-exports (editing's groove and written, five engine modules nothing used) (session)
