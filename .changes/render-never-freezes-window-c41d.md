---
"session-engine" = "patch"
"daw-extension" = "patch"
---

A long engine action — MIX_MEASURE, MIX_RENDER, a first LISTEN, LISTEN_AB — no longer stops the engine answering anything else while it runs: the window's view calls and transport, and every other action, answer in milliseconds while it renders, so the window no longer goes gray. A sync action asked over the window's link (`EngineView/run`) or through the daw extension (`DAW_RUN`) runs on a worker of its own rather than inside the connection that asked for it (session)
