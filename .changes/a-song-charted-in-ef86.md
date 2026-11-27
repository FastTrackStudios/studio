---
"chart-import" = "patch"
"daw-extension" = "patch"
"keyflow-extension" = "patch"
"keyflow-session" = "patch"
---

A song charted in a .kf gets that chart in its session when Studio opens it (one undo step, never over a chart the session has); `just import-charts` does the same over a whole setlist; the desktop chart pane types a session's written chart in, so it draws the real chart, not a bare section outline.
