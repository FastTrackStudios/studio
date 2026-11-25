---
"session-engine" = "minor"
---

Studio answers host-timeline/1 for every CLAP it loads: each instance is bound by the pump to its track's timeline (the track's MIDI items merged as the song plays them, and the tempo map), its revision moves only when that content changes, a `timeline` event goes to the plugins that subscribed, and a plugin's playback claim stops the engine sending it the track's own notes (session)
