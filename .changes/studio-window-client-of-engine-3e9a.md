---
"session-engine" = "minor"
"session-ui" = "minor"
"session-desktop" = "minor"
"studio-desktop" = "minor"
---

A window is always a client of its engine: on the desktop a separate engine process (the app's own binary run as `--session-engine`, or `studio engine`) attached to or spawned, replaced when it is of another build and started again — reopening what it had open — when it dies; on iOS the engine in the window's process reached through the same client over an in-memory link. Everything a window draws comes over the engine's view service (`EngineView`): the set, waveforms as the engine draws them, fine detail, automation lanes and take volume curves, hit overlays, marks, streaming state, playheads, and the window's edits go to the engine as one batch. `FTS_ENGINE=inprocess` is gone; new actions `SESSION_CLOSE`, `SESSION_JOIN`, `SESSION_SONG_SET`, `SESSION_PAGE`, `SESSION_SPAN`, `AUTOMATION_MENU`, `AUTOMATION_SET_MODE`, `ORGANIZE_PRESS`, `CHART_APPLY`, `VOCAL_RELEVEL`, `RIG_BROWSE` (session)
