---
"daw-extension" = "minor"
"engine-collab" = "minor"
"engine-core" = "minor"
"engine-domain" = "minor"
"engine-host" = "minor"
"engine-project" = "minor"
"fts-account" = "minor"
"keyflow-extension" = "minor"
"keyflow-session" = "minor"
"session" = "minor"
"session-format" = "minor"
"song-file" = "minor"
"studio-engine" = "minor"
---

Studio no longer uses architect's crdt: a session's file is read and written by the session crate itself (the same bytes, so every session and chart file made before opens), every edit kept as it is made; a chart session is shared live through live-doc; and a project no longer syncs through its org on Task (`studio share` is gone): song files sync through Drive
