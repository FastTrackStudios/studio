---
"daw-view" = "patch"
"studio-engine" = "patch"
---

An undo after a fast run of tempo edits takes back one step at a time: each ruler gesture (ARRANGE_* actions), TEMPO_SET and TEMPO_MAP_SET waits for the song's history to record the steps before it and its own, so a tempo then a meter are two steps and not one commit (#98)
