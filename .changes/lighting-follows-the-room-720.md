---
"daw-extension" = "minor"
"session" = "none"
"studio-app" = "none"
---

Ignition's lighting follows the room's session: a room member's engine opens what the leader loads and goes to their song, and its chart and lighting follow that session. `SESSION_FOLLOWED` answers on the engine's own MCP which session the chart and the lighting follow (its path, id and the URL its transport is led from), and Seam 2's room harness checks that a member's follows the leader's through a song change, a second setlist and a lone session.
