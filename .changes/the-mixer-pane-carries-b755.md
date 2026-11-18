---
"session-engine" = "patch"
"session-ui" = "patch"
"studio-arrangement" = "patch"
"studio-mixer" = "patch"
"studio-view-link" = "patch"
---

The mixer pane carries out the mixer's window actions: MIXER_VIEW, the racks switch and the mixer toggle (x) reach it, claimed from the engine by name; it draws an edit made in the arrangement as the engine's event lands, not at its next tick; a click on an item in the arrangement pane no longer panics its view (the read-back blocked inside the view's runtime) (session)
