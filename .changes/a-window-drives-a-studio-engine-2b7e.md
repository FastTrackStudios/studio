---
"session-engine" = "minor"
"session-ui" = "patch"
"session-desktop" = "minor"
"studio-desktop" = "patch"
---

A desktop window drives a Studio engine in another process with FTS_ENGINE=spawn or attach: it finds a running `studio engine` or spawns one in a process group of its own, attaches over its socket, runs its actions there and follows its set; the engine plays on when the window goes. Live's roll-on no longer jumps on from a song picked past its end; SESSION_TRANSPORT reads where the engine plays (session)
