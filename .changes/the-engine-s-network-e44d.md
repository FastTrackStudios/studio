---
"daw-extension" = "minor"
"engine-collab" = "minor"
"engine-host" = "minor"
"keyflow-extension" = "minor"
"room" = "minor"
"session" = "minor"
"studio-engine" = "minor"
---

The engine's network lanes admit only its owner (studio#362). `studio engine --listen` and `engine-host --engine` serve /vox and iroh behind the engine's door (`studio_engine::gate`, architect's PermissionsGate): a WebSocket is admitted only with the engine's token (a 0600 file, `~/.config/fts/engine-token`, which the engine's own clients present), an iroh peer only when the endpoint its handshake proved is one of the engine's devices (`engine-peers.json`, `--admit ID`); everyone else is refused. `engine-host --engine` binds loopback unless `--lan`. The desktop engine's session presence (port 4142) and the chart's session serve bind loopback unless asked (`FTS_DAW_LEAD_BIND`, `bind`). A set shared outside a room is served behind a door too: its ticket carries a pass minted for it (`fts-session:<endpoint>/<set>/<pass>`).
