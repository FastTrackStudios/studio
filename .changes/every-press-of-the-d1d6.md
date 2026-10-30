---
"session-desktop" = "patch"
"session-engine" = "patch"
"session-ui" = "patch"
---

Every press of the track panel, mixer strips, transport, toolbar options, mode and phase pickers and the dock's bar runs a named action, so the engine's watch, MCP and the studio CLI see and can do what a click does; new TRACK_LANE_STEP, SESSION_STOP, SESSION_GO_TO_END, SESSION_LOOP, SESSION_SEEK, SESSION_PLAY_FROM, SESSION_OPTION; a guard test keeps it so (session)
