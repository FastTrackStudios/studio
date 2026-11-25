---
"session-engine" = "patch"
---

A still song costs the engine almost nothing: SIGNAL_FEED reads Signal's revisions and meters without waiting for a render (15 µs, was 481 ms for 24 instances under render), the clap pump takes a plugin's lock only when it asked for its main thread or has events due, user automation edits are looked for only when the song moved, and autosave reads only the plugins whose state may have changed (span `autosave.look`) (session)
