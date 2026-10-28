---
"session-engine" = "patch"
"session-ui" = "patch"
"session-desktop" = "patch"
---

The window's link to its engine: a call that runs long fails on its own instead of dropping the link (actions get two minutes); edits the window made for a song that is no longer current are refused, not applied to the current one; `session-desktop --session-engine` runs the engine; the engine's socket lets go of a client that hangs up; New Project and Join say an immediate refusal; `DRUMS_REVERT` with no window says so at once; `SESSION_SONG_SET` with `to` and `remove` removes the moved song; `RIG_BROWSE` takes a track's name or path; `CHART_APPLY` runs inside the engine's runtime; `SESSION_NEW` says the song opens in the background; an engine with no window keeps its own streaming state (session)
