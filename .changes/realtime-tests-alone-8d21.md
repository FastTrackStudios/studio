---
"xtask-workflows" = "patch"
---

The engine's real-time tests run alone again, with two retries: their nextest overrides were lost when daw-standalone came back into the monorepo, so they played short on a loaded CI runner (studio#803) (xtask-workflows)
