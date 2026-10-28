---
"session" = "minor"
"session-engine" = "minor"
"session-desktop" = "minor"
"studio-desktop" = "minor"
---

`studio ingest` puts songs from this machine into an org's Task library, ready to open anywhere: a Live set (split into songs), song folders, `.session`/`.RPP` files or an album folder are prepared (proxies, peaks, reference), uploaded to roots at `session/<slug>` on the org's media volume (only what the server lacks, streamed and resumable), and given their setlist, projects and, with `--link`, a live link. The same ingest is the welcome screen's Publish to Task card and the Action List's `SESSION_INGEST`; the library also finds songs on older album roots (`Projects/<Album>/<Song>`) (session)
