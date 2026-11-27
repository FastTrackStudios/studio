---
"daw-extension" = "minor"
"keyflow-extension" = "minor"
"keyflow-session" = "minor"
"studio-app" = "minor"
"studio-engine" = "minor"
---

A session with no chart shows the chart Keyflow generates from its markers (its sections and measure numbers), followed at the playhead; a written chart beside the song is kept. New SESSION_CHART says which chart a song shows. A song with no tempo points no longer has its session retimed in 4/4.

SESSION_CHART_AT's `source` for a song with no `.kf` is now `session` (it was `markers`): its bars and sections are the chart its session shows. DAW_STATE's `session` says the `.kf` beside the song (`chart`), and KEYFLOW_SESSION_FOLLOW no longer fails when a written chart cannot be typed in (`seed_error`).
