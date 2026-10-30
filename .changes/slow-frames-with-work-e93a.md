---
"session-ui" = "patch"
---

A late frame is exported as a `ui.frame.slow` trace only when something ran on the paint thread in its gap; how often and when frames are late stays in the `ui.frame.missed` / `ui.frame.interval` metrics. Idle redraws were ~600 traces a minute (session)
