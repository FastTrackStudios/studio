---
"guitar" = "minor"
"signal-desktop" = "minor"
"signal-extension" = "minor"
"signal-proto" = "minor"
---

Signal's 43 commands are one architect Action List: keys, MIDI, REAPER and a model run them by id. `signal-desktop --engine --mcp` (or `SIGNAL_MCP=1`) serves them over MCP on loopback, behind a token it generates and writes to `~/.config/fts/signal-mcp.json`.
