---
"daw-view-kit" = "none"
"studio-panels" = "none"
"arrangement-view" = "none"
"mixer-view" = "none"
"session-desktop" = "none"
"studio-app" = "none"
"studio-arrangement" = "none"
"studio-mixer" = "none"
"studio-lyrics" = "none"
"studio-navigator" = "none"
"studio-view-link" = "none"
---

Studio's app panels (the shell and its bars, the chart and its editor, collaboration, the performer screen, the lyrics, the transport bar, the navigator, Setup, the record view, the browser, versions, the loading screen, the web page and the app menu) moved out of daw-view-kit into a new crate, studio-panels; the kit no longer depends on app-shell, editor-view, editor-state, editor-crdt or the setlist and collaboration code (session)
