---
"studio-desktop" = "minor"
"session-engine" = "none"
"session-ui" = "none"
"session-daw-web" = "none"
"editor" = "none"
"session-extension" = "none"
---

Session lives in Studio now: the `session-desktop` crate is gone — its DAW view, its `--session-engine`, its `studio` commands and its browser remote are studio-desktop's (`src/session/`), and `just app` opens Studio's DAW view; the separate Session binary, its macOS `.pkg` and its old headless `--engine` (with its Recording Mode and WebView workspaces) are retired (session)
