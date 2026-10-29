---
"session" = "patch"
"session-engine" = "minor"
"session-ui" = "minor"
"session-desktop" = "patch"
---

The drum workflow is traced end to end and can be driven from outside: a key or a click (`window.action`, `window.key`) is one trace with what it asks of the engine (the comp, the quantize, their stages) and the edit's recording after it (`song.step`: the MIDI views following, the song's doc written); the song's load and its sync bridge, each step a span. SESSION_MODE and SESSION_PHASE reach the window again since it runs in its own process; SESSION_WINDOW says what it shows; with several windows following the engine, SESSION_WINDOW_TARGET picks which one the window actions drive (FTS_WINDOW_NAME) (session)
