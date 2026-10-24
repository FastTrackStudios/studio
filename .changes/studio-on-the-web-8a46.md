---
"app-shell" = "minor"
"session" = "minor"
"session-daw-web" = "minor"
"studio-desktop" = "minor"
---

Studio on the web finds Task and fts-auth from /config.json at boot (task_url, auth_url), falling back to the page's own origin; Session takes share links given as paths on the configured Task, and Signal's rig engine stays on the page's own origin (studio)
