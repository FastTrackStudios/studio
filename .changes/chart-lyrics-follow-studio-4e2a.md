---
"setlists" = "minor"
"studio-chart" = "minor"
"studio-lyrics" = "minor"
"studio-architect" = "minor"
---

Chart and Lyrics, Architect extensions of their own (`studio-chart`, `studio-lyrics`), and one playhead for a setlist: the setlist's running order is its source on Architect's playhead, moved by a running Studio engine's transport (read only) or by Architect's own clock, and Chart (the song playing's engraved chart, bar and beat followed, an arrangement's own chart when the setlist plays one), Lyrics (its timed lyrics, Session's stage display) and Setlist (the running order, the song playing marked, a row plays it) follow it; `studio.setlists` gives others `now` and `seek` (Lighting follows the same clock), and a setlist note's favourites are Setlist, Chart, Lyrics and Lighting (session)
