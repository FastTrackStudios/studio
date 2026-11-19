---
"dynamic-template" = "patch"
"session" = "patch"
"session-engine" = "minor"
---

A REAPER project imports as a song the way an Ableton set does (`prepare "Song.RPP" --as "Title"`): its tempo map and meters are kept, its own regions become the sections (a project marked up in REAPER has them on its first ruler lane, where Session keeps the song; a note such as `Kick in` was made a section instead), with the SONG region and SONGSTART/SONGEND placed over them, and its markers stay marks. A record with its separation (the mix, its stems, a click) comes in with the stems in instrument folders, unmixed, instead of filed as reference material. Every prepared song gets its stereo reference in a top-level `Reference` folder, and one that came with every stem muted under the mix is heard by the reference with the stems ready behind the Stems/Reference switch. `prepare --recording "Song.mp3" --as "Title" [--bpm N] [--meter 3/4]` makes a session of a recording alone, which plays by the switch either way. `song-report` says what a song holds (tracks, tempo changes, sections, markers) and, with `--levels`, how its reference and its stems compare (session)
