---
"ignition-pane" = "patch"
"ignition-live-ui" = "patch"
"ignition" = "patch"
"studio-app" = "patch"
"daw-extension" = "patch"
---

Ignition's lighting no longer calls itself a room, so "room" means only Studio's room (ADR 0003): the 3D view is the visualizer (the `visualizer` view; `IGNITION_SESSION_VIZ`, `IGNITION_VIZ_CUES`, `IGNITION_VIZ_CUE`, `IGNITION_VIZ_GO`, `IGNITION_VIZ_STATE`), and the programming actions say venue for the place the rig hangs in. The old `IGNITION_SESSION_ROOM` and `IGNITION_ROOM_*` ids still work, as presets of the new ones.
