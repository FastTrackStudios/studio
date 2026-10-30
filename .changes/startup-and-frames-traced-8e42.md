---
"session-engine" = "minor"
"session-ui" = "minor"
"session-desktop" = "patch"
"studio-desktop" = "patch"
---

What the app does and how fast is traced and measured: the engine's start is one trace (`engine.start` → `set.open` → a `project.load` per song, with its build, its fetch and the slow calls under it), a window's read of a song (`song.read_back`) and a song's waveforms (`song.pictures`) are traces of their own; the engine's audio health and the window's frames are metrics (`ui.frames_per_second` and its 1% low, `ui.frame.interval`, `ui.frame.missed` against 120 and 60 Hz), and a frame that misses 120 Hz is a trace of what the window did in it (`ui.frame.slow`). A window attaching only waits for its engine however long it takes (session)
