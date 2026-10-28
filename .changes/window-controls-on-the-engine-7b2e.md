---
"session-engine" = "patch"
"session-ui" = "patch"
"session-desktop" = "patch"
---

A window's controls act on its engine: ⌘K lists and runs the engine's actions (`SESSION_ACTIONS`), the Versions panel saves and lists through it (`SESSION_VERSIONS`), the bottom bar shows the engine's jobs (a `jobs` stream), and stop behaviour, render ahead, grouping (`SESSION_GROUPING`) and the multitracks (`SESSION_MULTITRACKS`) are set where they take effect; "play waits" and how the songs are heard come from the engine's streams (session)
