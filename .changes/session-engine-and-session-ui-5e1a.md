---
"studio-engine" = "minor"
"session-ui" = "minor"
"engine-host" = "patch"
"session-reaper-xtask" = "none"
"app-shell" = "none"
"dynamic-template" = "none"
"daw-ui" = "none"
"patch-list" = "none"
---

Session's engine and window are two crates, cut from session-daw: studio-engine (the set, the Action List, opening, streaming, editing; draws nothing) and session-ui (the window, a view of it). The Action List is complete with no window; a window's own actions join it when the window attaches. The edit cursor and time selection are engine state, and Live mode's roll-on is the engine's (session)
